//! 在线安装 Minecraft：版本 JSON → 客户端 jar → 依赖库 → 资源文件。
//!
//! 与原生版一致的三条规则：
//! - 版本 JSON 走镜像，失败回退到清单里的官方地址；
//! - 库文件按 `downloads.artifact.path` 落位，没有该字段时用 maven 坐标现拼；
//! - BMCLAPI 的资源对象路径是 `/assets/{hash 前两位}/{hash}`（**没有** `objects/` 段）。

use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

use crate::config::Config;
use crate::download;
use crate::launch;
use crate::manifest;
use crate::paths;

/// 安装进度上报器：一个任务内的多个阶段共用一个实例。
#[derive(Clone)]
pub struct Progress {
    inner: Arc<Inner>,
}

struct Inner {
    app: Option<AppHandle>,
    task: String,
    total: AtomicUsize,
    done: AtomicUsize,
    base: AtomicU64, // 阶段起点（千分比）
    span: AtomicU64, // 阶段跨度（千分比）
    label: std::sync::Mutex<String>,
}

impl Progress {
    pub fn new(app: Option<AppHandle>, task: impl Into<String>) -> Self {
        Self {
            inner: Arc::new(Inner {
                app,
                task: task.into(),
                total: AtomicUsize::new(0),
                done: AtomicUsize::new(0),
                base: AtomicU64::new(0),
                span: AtomicU64::new(1000),
                label: std::sync::Mutex::new(String::new()),
            }),
        }
    }

    /// 进入新阶段，并声明本阶段要处理的项目数。
    pub fn stage(&self, label: &str, total: usize, base: f64, span: f64) {
        self.inner.done.store(0, Ordering::Relaxed);
        self.inner.total.store(total, Ordering::Relaxed);
        self.inner
            .base
            .store((base * 1000.0) as u64, Ordering::Relaxed);
        self.inner
            .span
            .store((span * 1000.0) as u64, Ordering::Relaxed);
        *self.inner.label.lock().unwrap() = label.to_string();
        self.emit(label);
    }

    /// 完成一个项目。`force` 为 false 时按步长节流，避免几千次事件淹没前端。
    pub fn tick(&self, force: bool) {
        let done = self.inner.done.fetch_add(1, Ordering::Relaxed) + 1;
        let total = self.inner.total.load(Ordering::Relaxed).max(1);
        if force || done % 16 == 0 || done == total {
            let label = self.inner.label.lock().unwrap().clone();
            self.emit(&format!("{label} {done}/{total}"));
        }
    }

    pub fn note(&self, label: &str) {
        *self.inner.label.lock().unwrap() = label.to_string();
        self.emit(label);
    }

    fn emit(&self, label: &str) {
        let Some(app) = &self.inner.app else { return };
        let done = self.inner.done.load(Ordering::Relaxed) as f64;
        let total = self.inner.total.load(Ordering::Relaxed).max(1) as f64;
        let base = self.inner.base.load(Ordering::Relaxed) as f64 / 1000.0;
        let span = self.inner.span.load(Ordering::Relaxed) as f64 / 1000.0;
        let frac = (base + span * (done / total)).clamp(0.0, 1.0);
        let _ = app.emit(
            "progress",
            serde_json::json!({
                "task": self.inner.task,
                "label": label,
                "frac": frac,
                "done": done as u64,
                "total": total as u64,
            }),
        );
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct InstallOutcome {
    pub version_id: String,
    pub downloaded: usize,
    pub skipped: usize,
    pub failed: Vec<String>,
    pub assets: usize,
}

/// 一个待下载项。
struct Job {
    dest: PathBuf,
    urls: Vec<String>,
    sha1: Option<String>,
}

/// 安装（补齐）某个版本需要的全部文件。已存在的文件会跳过，因此可以反复调用。
pub async fn install_version(
    app: Option<AppHandle>,
    version_id: &str,
    cfg: &Config,
) -> Result<InstallOutcome, String> {
    let prog = Progress::new(app, format!("install:{version_id}"));
    prog.note(&format!("读取 {version_id} 的版本信息"));
    paths::ensure_dirs();

    let chain = manifest::version_chain(version_id, cfg).await?;
    let merged = manifest::merge_chain(&chain);
    let jar_id = manifest::client_jar_id(&chain, &merged, version_id);

    // ---------- 1. 客户端 jar ----------
    prog.stage("下载客户端主程序", 1, 0.02, 0.10);
    let jar_path = paths::paths()
        .versions_dir
        .join(&jar_id)
        .join(format!("{jar_id}.jar"));
    let mut jobs: Vec<Job> = Vec::new();
    if !is_usable(&jar_path) {
        let mirror = format!("{}/versions/{jar_id}/{jar_id}.jar", cfg.download_base());
        let mut urls = vec![mirror];
        let (official, sha1) = client_jar_source(&chain, &jar_id);
        if let Some(u) = official {
            urls.push(u);
        }
        jobs.push(Job {
            dest: jar_path.clone(),
            urls,
            sha1,
        });
    }
    let jar_jobs = jobs.len();
    let jar_result = run_jobs(&jobs, 1).await?;
    if !is_usable(&jar_path) {
        return Err(format!(
            "客户端主程序下载失败：{}\n（若这是整合包实例，请先安装它继承的原版版本）",
            if jar_result.2.is_empty() {
                "文件没有落盘".to_string()
            } else {
                jar_result.2.join("；")
            }
        ));
    }
    prog.tick(true);
    jobs.clear();

    // ---------- 2. 依赖库 ----------
    let libs = manifest::libraries(&merged);
    for lib in &libs {
        collect_library(lib, cfg, &mut jobs);
    }
    let lib_total = jobs.len();
    prog.stage("下载依赖库", lib_total, 0.12, 0.48);
    let lib_result = run_jobs(&jobs, 8).await;
    let (mut downloaded, mut skipped, mut failed) = (jar_jobs, 0usize, Vec::new());
    match lib_result {
        Ok(stats) => {
            downloaded += stats.0;
            skipped += stats.1;
            failed.extend(stats.2);
        }
        Err(e) => failed.push(e),
    }

    // ---------- 3. 资源文件 ----------
    prog.stage("下载资源文件", 1, 0.60, 0.38);
    let assets = match ensure_assets(&merged, cfg, &prog).await {
        Ok(n) => n,
        Err(e) => {
            failed.push(format!("资源文件：{e}"));
            0
        }
    };

    prog.note("安装完成");
    Ok(InstallOutcome {
        version_id: version_id.to_string(),
        downloaded,
        skipped,
        failed,
        assets,
    })
}

/// 并发执行下载任务，返回（成功数、跳过数、失败明细）。
async fn run_jobs(jobs: &[Job], concurrency: usize) -> Result<(usize, usize, Vec<String>), String> {
    let sem = Arc::new(tokio::sync::Semaphore::new(concurrency.max(1)));
    let mut handles = Vec::new();
    for job in jobs {
        let sem = sem.clone();
        let dest = job.dest.clone();
        let urls = job.urls.clone();
        let sha1 = job.sha1.clone();
        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await.ok()?;
            if is_usable(&dest) {
                return Some((true, true, String::new()));
            }
            match download::download_any(&urls, &dest, sha1.as_deref()).await {
                Ok(()) => Some((true, false, String::new())),
                Err(e) => Some((false, false, e)),
            }
        }));
    }
    let (mut ok, mut skip, mut fail) = (0usize, 0usize, Vec::new());
    for h in handles {
        match h.await {
            Ok(Some((true, true, _))) => skip += 1,
            Ok(Some((true, false, _))) => ok += 1,
            Ok(Some((false, _, e))) => fail.push(e),
            _ => {}
        }
    }
    Ok((ok, skip, fail))
}

/// 把某个 library 需要下载的文件加进任务表。
fn collect_library(lib: &Value, cfg: &Config, out: &mut Vec<Job>) {
    if !launch::rules_allow(lib.get("rules").and_then(|v| v.as_array())) {
        return;
    }
    let base = cfg.download_base();
    let libs_dir = paths::paths().libraries_dir.clone();
    let name = lib.get("name").and_then(|v| v.as_str()).unwrap_or("");

    let mut has_artifact = false;
    if let Some(art) = lib.get("downloads").and_then(|d| d.get("artifact")) {
        let rel = art
            .get("path")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| launch::maven_path(name));
        if !rel.is_empty() {
            has_artifact = true;
            let mut urls = vec![format!("{base}/libraries/{rel}")];
            if let Some(u) = art
                .get("url")
                .and_then(|v| v.as_str())
                .filter(|u| !u.is_empty())
            {
                urls.push(u.to_string());
            }
            out.push(Job {
                dest: libs_dir.join(&rel),
                urls,
                sha1: art
                    .get("sha1")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
            });
        }
    }

    // 没有 downloads 段的库（Forge / Fabric 自带的 maven 库）按坐标拼镜像地址
    if !has_artifact && !name.is_empty() {
        let rel = launch::maven_path(name);
        if !rel.is_empty() {
            out.push(Job {
                dest: libs_dir.join(&rel),
                urls: vec![format!("{base}/libraries/{rel}")],
                sha1: None,
            });
        }
    }

    // 老版本的 natives 分类器
    if let Some(natives) = lib.get("natives") {
        let key = natives
            .get(launch::os_name())
            .and_then(|v| v.as_str())
            .map(|s| s.replace("${arch}", "64"));
        if let Some(classifier) = key {
            if let Some(cl) = lib
                .get("downloads")
                .and_then(|d| d.get("classifiers"))
                .and_then(|c| c.get(&classifier))
            {
                let rel = cl
                    .get("path")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| {
                        let mut p = launch::maven_path(name);
                        if !classifier.is_empty() {
                            p = p.replace(
                                &format!("{}.jar", name.rsplit(':').next().unwrap_or("")),
                                &format!("{classifier}.jar", classifier = classifier),
                            );
                        }
                        p
                    });
                if !rel.is_empty() {
                    let mut urls = vec![format!("{base}/libraries/{rel}")];
                    if let Some(u) = cl
                        .get("url")
                        .and_then(|v| v.as_str())
                        .filter(|u| !u.is_empty())
                    {
                        urls.push(u.to_string());
                    }
                    out.push(Job {
                        dest: libs_dir.join(&rel),
                        urls,
                        sha1: cl
                            .get("sha1")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                    });
                }
            }
        }
    }
}

/// 下载资源索引与全部资源对象，返回本次实际下载的对象数。
async fn ensure_assets(merged: &Value, cfg: &Config, prog: &Progress) -> Result<usize, String> {
    let Some(index) = merged.get("assetIndex") else {
        return Ok(0);
    };
    let id = index
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("legacy")
        .to_string();
    let assets_dir = paths::paths().assets_dir.clone();
    let index_file = assets_dir.join("indexes").join(format!("{id}.json"));

    if !is_usable(&index_file) {
        let mut urls = vec![format!("{}/assets/indexes/{id}.json", cfg.download_base())];
        if let Some(u) = index
            .get("url")
            .and_then(|v| v.as_str())
            .filter(|u| !u.is_empty())
        {
            urls.push(u.to_string());
        }
        download::download_any(&urls, &index_file, None)
            .await
            .map_err(|e| format!("资源索引 {id} 下载失败：{e}"))?;
    }

    let raw = tokio::fs::read_to_string(&index_file)
        .await
        .map_err(|e| format!("读取资源索引失败：{e}"))?;
    let parsed: Value = serde_json::from_str(&raw).map_err(|e| format!("资源索引解析失败：{e}"))?;
    let Some(objects) = parsed.get("objects").and_then(|v| v.as_object()) else {
        return Ok(0);
    };

    let base = cfg.download_base();
    let mut jobs: Vec<Job> = Vec::new();
    for (_, meta) in objects {
        let Some(hash) = meta.get("hash").and_then(|v| v.as_str()) else {
            continue;
        };
        if hash.len() < 2 {
            continue;
        }
        let dest = assets_dir.join("objects").join(&hash[0..2]).join(hash);
        if is_usable(&dest) {
            continue;
        }
        jobs.push(Job {
            dest,
            urls: vec![
                // BMCLAPI 的对象路径没有 objects 段
                format!("{base}/assets/{}/{hash}", &hash[0..2]),
                format!(
                    "https://resources.download.minecraft.net/{}/{hash}",
                    &hash[0..2]
                ),
            ],
            sha1: Some(hash.to_string()),
        });
    }

    let total = jobs.len();
    if total == 0 {
        return Ok(0);
    }
    prog.stage("下载资源文件", total, 0.60, 0.38);
    let sem = Arc::new(tokio::sync::Semaphore::new(16));
    let ok = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();
    for job in jobs {
        let sem = sem.clone();
        let prog = prog.clone();
        let counter = ok.clone();
        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await;
            if download::download_any(&job.urls, &job.dest, job.sha1.as_deref())
                .await
                .is_ok()
            {
                counter.fetch_add(1, Ordering::Relaxed);
                prog.tick(false);
            }
        }));
    }
    for h in handles {
        let _ = h.await;
    }
    prog.tick(true);
    Ok(ok.load(Ordering::Relaxed))
}

fn client_jar_source(chain: &[Value], jar_id: &str) -> (Option<String>, Option<String>) {
    for json in chain {
        let id = json.get("id").and_then(|v| v.as_str()).unwrap_or("");
        if id != jar_id {
            continue;
        }
        if let Some(client) = json.get("downloads").and_then(|d| d.get("client")) {
            let url = client
                .get("url")
                .and_then(|v| v.as_str())
                .filter(|u| !u.is_empty())
                .map(|s| s.to_string());
            let sha1 = client
                .get("sha1")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            return (url, sha1);
        }
    }
    (None, None)
}

/// 文件存在且非空。
fn is_usable(p: &Path) -> bool {
    std::fs::metadata(p)
        .map(|m| m.is_file() && m.len() > 0)
        .unwrap_or(false)
}

// ---------------------------------------------------------------- 查询/清理

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct VersionDetail {
    pub id: String,
    pub main_class: String,
    pub jar_id: String,
    pub libraries: usize,
    pub missing_libraries: usize,
    pub jar_ok: bool,
    pub assets: String,
    pub java_major: u32,
    pub inherited_from: Option<String>,
}

pub async fn version_detail(version_id: &str, cfg: &Config) -> Result<VersionDetail, String> {
    let chain = manifest::version_chain(version_id, cfg).await?;
    let merged = manifest::merge_chain(&chain);
    let jar_id = manifest::client_jar_id(&chain, &merged, version_id);
    let jar_ok = manifest::existing_client_jar(&jar_id, version_id).is_some();
    Ok(VersionDetail {
        id: version_id.to_string(),
        main_class: merged
            .get("mainClass")
            .and_then(|v| v.as_str())
            .unwrap_or("net.minecraft.client.main.Main")
            .to_string(),
        jar_id: jar_id.clone(),
        libraries: manifest::libraries(&merged).len(),
        missing_libraries: launch::missing_classpath_files(&merged, &jar_id, version_id),
        jar_ok,
        assets: merged
            .get("assets")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        java_major: merged
            .get("javaVersion")
            .and_then(|v| v.get("majorVersion"))
            .and_then(|v| v.as_u64())
            .unwrap_or(8) as u32,
        inherited_from: merged
            .get("inheritsFrom")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    })
}

/// 删除版本目录（版本隔离的实例会连同 mods/存档一起删除）。
pub fn delete_version(version_id: &str) -> Result<(), String> {
    if version_id.trim().is_empty() || version_id.contains('/') || version_id.contains("..") {
        return Err("版本 id 不合法".into());
    }
    let dir = paths::paths().versions_dir.join(version_id);
    if !dir.is_dir() {
        return Err(format!("版本 {version_id} 不在本地"));
    }
    std::fs::remove_dir_all(&dir).map_err(|e| format!("删除失败：{e}"))
}
