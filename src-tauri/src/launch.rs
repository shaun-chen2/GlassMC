//! 启动命令构建。这里是整个启动器的技术核心，逻辑与 macOS 原生版一一对应：
//!
//! 1. `inheritsFrom` 深合并（见 manifest.rs）；
//! 2. 合并后按 rules 过滤库，客户端 jar 必须排在 classpath 第一位；
//! 3. jvm / game 参数分别展开占位符，再做「同名选项保留最后一个」去重；
//! 4. ignoreList 必须包含真实主 jar 名，否则 modlauncher 会把客户端 jar 当模组；
//! 5. `-XstartOnFirstThread` 只在 macOS 需要（GLFW 要求主线程启动）。

use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::account::Account;
use crate::config::Config;
use crate::java::JavaInstall;
use crate::manifest;
use crate::paths;

pub fn os_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "osx"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    }
}

pub fn arch_name() -> String {
    match std::env::consts::ARCH {
        "aarch64" => "arm64".into(),
        other => other.into(),
    }
}

pub fn cp_sep() -> &'static str {
    if cfg!(target_os = "windows") {
        ";"
    } else {
        ":"
    }
}

// ---------------------------------------------------------------- rules

/// 标准 rules 语义：默认不允许；按顺序应用匹配到的规则，最后一条匹配规则决定结果。
/// 带 `features` 的规则一律视为不匹配（本启动器不启用 demo / quickPlay）。
pub fn rules_allow(rules: Option<&Vec<Value>>) -> bool {
    let Some(rules) = rules else { return true };
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    let mut matched = false;
    let cur_arch = arch_name();
    for rule in rules {
        let Some(obj) = rule.as_object() else { continue };
        if obj.contains_key("features") {
            continue;
        }
        let mut ok = true;
        if let Some(os) = obj.get("os").and_then(|v| v.as_object()) {
            if let Some(name) = os.get("name").and_then(|v| v.as_str()) {
                let name = name.to_lowercase();
                let here = os_name();
                let hit = match here {
                    "osx" => name == "osx" || name == "macos" || name == "os x",
                    other => name == other,
                };
                if !hit {
                    ok = false;
                }
            }
            if ok {
                if let Some(arch) = os.get("arch").and_then(|v| v.as_str()) {
                    let arch = arch.to_lowercase();
                    let hit = arch == cur_arch
                        || (arch == "x86" && cur_arch == "x86_64")
                        || (arch == "arm64" && cur_arch == "arm64");
                    if !hit {
                        ok = false;
                    }
                }
            }
        }
        if ok {
            matched = true;
            allowed = obj
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("allow")
                == "allow";
        }
    }
    matched && allowed
}

fn flatten_arguments(arr: &[Value]) -> Vec<String> {
    let mut out = Vec::new();
    for item in arr {
        if let Some(s) = item.as_str() {
            out.push(s.to_string());
            continue;
        }
        let Some(obj) = item.as_object() else { continue };
        if !rules_allow(obj.get("rules").and_then(|v| v.as_array())) {
            continue;
        }
        match obj.get("value") {
            Some(Value::String(s)) => out.push(s.clone()),
            Some(Value::Array(a)) => {
                for x in a {
                    if let Some(s) = x.as_str() {
                        out.push(s.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    out
}

// ---------------------------------------------------------------- 路径

/// Maven 坐标 → 相对路径。
pub fn maven_path(name: &str) -> String {
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() < 3 {
        return name.replace('.', "/");
    }
    let (group, artifact, version) = (parts[0], parts[1], parts[2]);
    let classifier = if parts.len() >= 4 && !parts[3].is_empty() {
        Some(parts[3])
    } else {
        None
    };
    let ext = if parts.len() >= 5 && !parts[4].is_empty() {
        parts[4]
    } else {
        "jar"
    };
    let file = match classifier {
        Some(c) => format!("{artifact}-{version}-{c}.{ext}"),
        None => format!("{artifact}-{version}.{ext}"),
    };
    format!(
        "{}/{}/{}/{}",
        group.replace('.', "/"),
        artifact,
        version,
        file
    )
}

fn lib_dir() -> PathBuf {
    paths::paths().libraries_dir.clone()
}

/// 拼 classpath：客户端 jar + 满足 rules 的库（去重、保持顺序）。
pub fn resolve_classpath(merged: &Value, jar_id: &str, version_id: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    if let Some(jar) = manifest::existing_client_jar(jar_id, version_id) {
        let s = jar.to_string_lossy().to_string();
        if seen.insert(s.clone()) {
            out.push(s);
        }
    }

    for lib in manifest::libraries(merged) {
        if !rules_allow(lib.get("rules").and_then(|v| v.as_array())) {
            continue;
        }
        let p = lib
            .get("downloads")
            .and_then(|d| d.get("artifact"))
            .and_then(|a| a.get("path"))
            .and_then(|v| v.as_str())
            .map(|s| lib_dir().join(s))
            .or_else(|| {
                lib.get("name")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|n| lib_dir().join(maven_path(n)))
            });
        if let Some(p) = p {
            let s = p.to_string_lossy().to_string();
            if seen.insert(s.clone()) {
                out.push(s);
            }
        }
    }
    out
}

/// 统计 classpath 里缺了多少文件（用于界面提示“还缺依赖”）。
pub fn missing_classpath_files(merged: &Value, jar_id: &str, version_id: &str) -> usize {
    resolve_classpath(merged, jar_id, version_id)
        .iter()
        .filter(|p| !Path::new(p).is_file())
        .count()
}

// ---------------------------------------------------------------- 参数

fn subst(raw: &str, map: &[(&str, String)]) -> String {
    let mut s = raw.to_string();
    for (k, v) in map {
        if s.contains(k) {
            s = s.replace(k, v);
        }
    }
    s
}


/// 去掉完全重复的 `--add-opens` / `--add-exports` / `--add-reads`。
/// 父子版本常把同一份模块开放声明各写一遍；值不同时都保留，值相同才丢。
fn dedupe_exact_pairs(args: Vec<String>) -> Vec<String> {
    const KEYS: [&str; 3] = ["--add-opens", "--add-exports", "--add-reads"];
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if KEYS.contains(&a.as_str()) && i + 1 < args.len() {
            let key = format!("{}={}", a, args[i + 1]);
            if seen.insert(key) {
                out.push(a.clone());
                out.push(args[i + 1].clone());
            }
            i += 2;
            continue;
        }
        out.push(a.clone());
        i += 1;
    }
    out
}

/// 同名单值选项保留最后一个（父子版本各写一份 -p / -cp 的情况）。
fn dedupe_options(args: Vec<String>) -> Vec<String> {
    const PAIR_KEYS: [&str; 5] = ["-p", "--module-path", "-cp", "-classpath", "--add-modules"];
    let mut slots: Vec<(Option<String>, Vec<String>)> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if PAIR_KEYS.contains(&a.as_str()) && i + 1 < args.len() {
            slots.push((Some(a.clone()), vec![a.clone(), args[i + 1].clone()]));
            i += 2;
            continue;
        }
        if a.starts_with("-D") {
            if let Some(eq) = a.find('=') {
                slots.push((Some(a[..eq].to_string()), vec![a.clone()]));
                i += 1;
                continue;
            }
        }
        if a == "-XstartOnFirstThread" {
            slots.push((Some(a.clone()), vec![a.clone()]));
            i += 1;
            continue;
        }
        slots.push((None, vec![a.clone()]));
        i += 1;
    }
    let mut keep = vec![true; slots.len()];
    let mut last: HashMap<String, usize> = HashMap::new();
    for (idx, (key, _)) in slots.iter().enumerate() {
        if let Some(k) = key {
            if let Some(&prev) = last.get(k) {
                keep[prev] = false;
            }
            last.insert(k.clone(), idx);
        }
    }
    let mut out = Vec::new();
    for (idx, (_, vals)) in slots.into_iter().enumerate() {
        if keep[idx] {
            out.extend(vals);
        }
    }
    out
}

/// 游戏参数去重：`--key value` 同名只保留最后一个。
fn dedupe_game_arguments(args: Vec<String>) -> Vec<String> {
    const FLAGS: [&str; 4] = ["--demo", "--fullscreen", "--disableMultiplayer", "--disableChat"];
    let mut slots: Vec<(Option<String>, Vec<String>)> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a.starts_with("--")
            && !FLAGS.contains(&a.as_str())
            && i + 1 < args.len()
            && !args[i + 1].starts_with("--")
        {
            slots.push((Some(a.clone()), vec![a.clone(), args[i + 1].clone()]));
            i += 2;
            continue;
        }
        slots.push((None, vec![a.clone()]));
        i += 1;
    }
    let mut keep = vec![true; slots.len()];
    let mut last: HashMap<String, usize> = HashMap::new();
    for (idx, (key, _)) in slots.iter().enumerate() {
        if let Some(k) = key {
            if let Some(&prev) = last.get(k) {
                keep[prev] = false;
            }
            last.insert(k.clone(), idx);
        }
    }
    let mut out = Vec::new();
    for (idx, (_, vals)) in slots.into_iter().enumerate() {
        if keep[idx] {
            out.extend(vals);
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
pub fn jvm_arguments(
    merged: &Value,
    natives: &str,
    classpath: &[String],
    version_id: &str,
    jar_id: &str,
    game_dir: &str,
    assets_id: &str,
    version_type: &str,
    max_memory_mb: u32,
) -> Vec<String> {
    let p = paths::paths();
    let args: Vec<String> = match merged
        .get("arguments")
        .and_then(|a| a.get("jvm"))
        .and_then(|v| v.as_array())
    {
        Some(arr) => flatten_arguments(arr),
        None => vec![
            "-Xss1M".to_string(),
            format!("-Djava.library.path={natives}"),
            "-Dminecraft.launcher.brand=GlassMC".to_string(),
            "-Dminecraft.launcher.version=1".to_string(),
        ],
    };

    let cp = classpath.join(cp_sep());
    let jar_file = format!("{jar_id}.jar");
    let map: Vec<(&str, String)> = vec![
        ("${natives_directory}", natives.to_string()),
        ("${classpath}", cp),
        ("${classpath_separator}", cp_sep().to_string()),
        ("${library_directory}", p.libraries_dir.to_string_lossy().to_string()),
        ("${primary_jar_name}", jar_file.clone()),
        (
            "${primary_jar}",
            manifest::client_jar_path(jar_id).to_string_lossy().to_string(),
        ),
        ("${launcher_name}", "GlassMC".into()),
        ("${launcher_version}", env!("CARGO_PKG_VERSION").into()),
        ("${game_directory}", game_dir.to_string()),
        ("${assets_root}", p.assets_dir.to_string_lossy().to_string()),
        ("${assets_index_name}", assets_id.to_string()),
        ("${version_type}", version_type.to_string()),
        // JVM 参数里的 ${version_name}.jar 指「主 jar 文件名」，
        // 对整合包实例（jar 名 != 版本名）必须用 jar 名，否则 ignoreList 失效
        ("${version_name}", jar_id.to_string()),
    ];

    let mut out: Vec<String> = args.iter().map(|a| subst(a, &map)).collect();
    out = dedupe_options(out);
    out = dedupe_exact_pairs(out);

    // ignoreList 必须包含真实的客户端 jar 文件名
    if let Some(i) = out.iter().position(|a| a.starts_with("-DignoreList=")) {
        let mut items: Vec<String> = out[i]
            .trim_start_matches("-DignoreList=")
            .split(',')
            .map(|s| s.to_string())
            .collect();
        for extra in [jar_file.clone(), format!("{version_id}.jar")] {
            if !items.contains(&extra) {
                items.push(extra);
            }
        }
        out[i] = format!("-DignoreList={}", items.join(","));
    }

    if !out.iter().any(|a| a.starts_with("-Xmx")) {
        out.insert(0, format!("-Xmx{}M", max_memory_mb.max(1024)));
    }
    // macOS 上 LWJGL/GLFW 必须在主线程启动
    if cfg!(target_os = "macos") && !out.iter().any(|a| a == "-XstartOnFirstThread") {
        out.push("-XstartOnFirstThread".into());
    }
    out
}

pub fn game_arguments(
    merged: &Value,
    account: &Account,
    assets_id: &str,
    version_id: &str,
    version_type: &str,
    game_dir: &str,
) -> Vec<String> {
    let args: Vec<String> = match merged
        .get("arguments")
        .and_then(|a| a.get("game"))
        .and_then(|v| v.as_array())
    {
        Some(arr) => flatten_arguments(arr),
        None => merged
            .get("minecraftArguments")
            .and_then(|v| v.as_str())
            .map(|s| s.split(' ').map(|x| x.to_string()).collect())
            .unwrap_or_default(),
    };

    let p = paths::paths();
    // 外置登录由 authlib-injector 接管，userType 必须是 mojang；
    // 微软正版账号必须 msa，否则拿不到档案与皮肤。
    let user_type = if account.is_microsoft() { "msa" } else { "mojang" };
    let token = account.access_token();
    let map: Vec<(&str, String)> = vec![
        ("${auth_player_name}", account.username.clone()),
        ("${version_name}", version_id.to_string()),
        ("${game_directory}", game_dir.to_string()),
        ("${assets_root}", p.assets_dir.to_string_lossy().to_string()),
        ("${assets_index_name}", assets_id.to_string()),
        ("${auth_uuid}", account.auth_uuid()),
        ("${auth_access_token}", token.clone()),
        ("${auth_session}", token),
        ("${user_type}", user_type.to_string()),
        ("${version_type}", version_type.to_string()),
        ("${launcher_name}", "GlassMC".into()),
        ("${launcher_version}", env!("CARGO_PKG_VERSION").into()),
        ("${primary_jar_name}", format!("{version_id}.jar")),
        ("${clientid}", String::new()),
        ("${auth_xuid}", String::new()),
        ("${user_properties}", "{}".into()),
        ("${quickPlayPath}", String::new()),
        ("${quickPlaySingleplayer}", String::new()),
        ("${quickPlayMultiplayer}", String::new()),
        ("${quickPlayRealms}", String::new()),
    ];

    let mut out: Vec<String> = args.iter().map(|a| subst(a, &map)).collect();
    // 极少数版本会留下未知占位符，直接丢掉，避免游戏解析参数失败
    out.retain(|a| !a.contains("${"));
    dedupe_game_arguments(out)
}

fn apply_window_size(mut args: Vec<String>, cfg: &Config) -> Vec<String> {
    let set = |args: &mut Vec<String>, flag: &str, value: u32| {
        if value == 0 {
            return;
        }
        if let Some(i) = args.iter().position(|a| a == flag) {
            if i + 1 < args.len() {
                args[i + 1] = value.to_string();
            }
        } else {
            args.push(flag.to_string());
            args.push(value.to_string());
        }
    };
    set(&mut args, "--width", cfg.window_width);
    set(&mut args, "--height", cfg.window_height);
    args
}

// ---------------------------------------------------------------- natives

/// 旧版（1.12-）原生库需要解压到 natives 目录。1.13+ 的 natives 以 classifier
/// 库的形式直接在 classpath 里，不需要解压。
pub fn extract_natives(merged: &Value, natives_dir: &Path) {
    let _ = std::fs::create_dir_all(natives_dir);
    for lib in manifest::libraries(merged) {
        if !rules_allow(lib.get("rules").and_then(|v| v.as_array())) {
            continue;
        }
        let Some(name) = lib.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(natives) = lib.get("natives").and_then(|v| v.as_object()) else {
            continue;
        };
        let classifier = natives
            .get("osx")
            .or_else(|| natives.get("macos"))
            .and_then(|v| {
                // 值里可能带 ${arch}
                v.as_str().map(|s| s.replace("${arch}", if cfg!(target_pointer_width = "64") { "64" } else { "32" }))
            });
        let Some(classifier) = classifier else { continue };
        let jar = lib_dir().join(maven_path(&format!("{name}:{classifier}")));
        if !jar.is_file() {
            continue;
        }
        let marker = natives_dir.join(format!(".done-{}", jar.file_name().unwrap_or_default().to_string_lossy()));
        if marker.is_file() {
            continue;
        }
        if unzip_into(&jar, natives_dir).is_ok() {
            let _ = std::fs::write(&marker, b"");
        }
    }
}

fn unzip_into(jar: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(jar).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        let Some(rel) = entry.enclosed_name() else { continue };
        // 只要原生库，不要 META-INF 之类
        let ext = rel.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !matches!(ext, "dylib" | "jnilib" | "so" | "dll") {
            continue;
        }
        let out = dest.join(rel.file_name().unwrap_or_default());
        if let Ok(mut f) = std::fs::File::create(&out) {
            let _ = std::io::copy(&mut entry, &mut f);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- 组装

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BuiltCommand {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub version_id: String,
    pub java_version: u32,
    pub max_memory_mb: u32,
    pub classpath_count: usize,
    pub missing_files: usize,
}

/// HMCL 实例级设置里的内存要求（MB）。
fn instance_max_memory_mb(version_id: &str) -> Option<u32> {
    let p = paths::paths()
        .versions_dir
        .join(version_id)
        .join(".hmcl/config/instance-game-settings.json");
    let s = std::fs::read_to_string(p).ok()?;
    let v: Value = serde_json::from_str(&s).ok()?;
    v.get("maxMemory")
        .and_then(|x| x.as_u64())
        .filter(|x| *x > 0)
        .map(|x| x as u32)
}

pub async fn build_command(
    version_id: &str,
    account: &Account,
    java: &JavaInstall,
    cfg: &Config,
) -> Result<BuiltCommand, String> {
    if !Path::new(&java.path).is_file() {
        return Err(format!("Java 不可用：{}", java.path));
    }

    let chain = manifest::version_chain(version_id, cfg).await?;
    let merged = manifest::merge_chain(&chain);
    let jar_id = manifest::client_jar_id(&chain, &merged, version_id);

    if let Some(major) = merged
        .get("javaVersion")
        .and_then(|v| v.get("majorVersion"))
        .and_then(|v| v.as_u64())
    {
        if major as u32 > java.version {
            return Err(format!(
                "该版本需要 Java {major}，当前选中的是 Java {}（可在设置里换一个）",
                java.version
            ));
        }
    }

    let mut classpath = resolve_classpath(&merged, &jar_id, version_id);
    let jar = manifest::existing_client_jar(&jar_id, version_id).ok_or_else(|| {
        format!("缺少客户端 jar：versions/{jar_id}/{jar_id}.jar —— 请先在「安装」页装好这个版本")
    })?;
    let jar_str = jar.to_string_lossy().to_string();
    classpath.retain(|p| p != &jar_str);
    classpath.insert(0, jar_str.clone());
    let missing = classpath.iter().filter(|p| !Path::new(p).is_file()).count();

    let work_dir = manifest::game_directory(version_id);
    std::fs::create_dir_all(&work_dir).map_err(|e| format!("创建游戏目录失败：{e}"))?;

    let natives_dir = paths::paths().natives_dir.join(version_id);
    std::fs::create_dir_all(&natives_dir).ok();
    extract_natives(&merged, &natives_dir);

    let assets_id = merged
        .get("assets")
        .and_then(|v| v.as_str())
        .or_else(|| {
            merged
                .get("assetIndex")
                .and_then(|v| v.get("id"))
                .and_then(|v| v.as_str())
        })
        .unwrap_or("legacy")
        .to_string();
    let version_type = merged
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("release")
        .to_string();
    let main_class = merged
        .get("mainClass")
        .and_then(|v| v.as_str())
        .unwrap_or("net.minecraft.client.main.Main")
        .to_string();

    // 内存取「全局设置」与「实例自带 HMCL 设置」的较大值，大型整合包才不会 OOM
    let setting_mb = (cfg.memory_gb * 1024.0).round() as u32;
    let max_mb = setting_mb.max(instance_max_memory_mb(version_id).unwrap_or(0));

    let mut args: Vec<String> = jvm_arguments(
        &merged,
        &natives_dir.to_string_lossy(),
        &classpath,
        version_id,
        &jar_id,
        &work_dir.to_string_lossy(),
        &assets_id,
        &version_type,
        max_mb,
    )
    .to_vec();

    // 版本 JSON 的 jvm 参数通常已含 -cp，没有时才补
    let has_cp = args.iter().any(|a| a == "-cp" || a == "-classpath");
    if !has_cp {
        args.push("-cp".into());
        args.push(classpath.join(cp_sep()));
    }

    // 外置登录：必须挂 authlib-injector，否则客户端会去问 Mojang 要皮肤
    if account.kind == "yggdrasil" {
        if let Some(sid) = account.auth_server_id.as_deref() {
            if let Some(srv) = cfg.auth_servers.iter().find(|s| s.id == sid) {
                let jar = match crate::injector::local() {
                    Some(j) => Some(j),
                    None => crate::injector::ensure(false).await.ok(),
                };
                match jar {
                    Some(jar) => {
                        args.push(crate::injector::agent_arg(&jar, &srv.normalized_base()));
                        println!("[launch] 已挂载 authlib-injector：{}", srv.normalized_base());
                    }
                    None => println!("[launch] authlib-injector 不可用，外置登录皮肤无法同步"),
                }
            }
        }
    }

    // 自定义 JVM 参数放在主类之前，避免被当成游戏参数
    for extra in cfg
        .extra_jvm_args
        .split([' ', '\n', '\t'])
        .filter(|s| !s.is_empty())
    {
        args.push(extra.to_string());
    }

    args.push(main_class);

    let game_args = game_arguments(
        &merged,
        account,
        &assets_id,
        version_id,
        &version_type,
        &work_dir.to_string_lossy(),
    );
    args.extend(apply_window_size(game_args, cfg));

    Ok(BuiltCommand {
        program: java.path.clone(),
        args,
        cwd: work_dir.to_string_lossy().to_string(),
        version_id: version_id.to_string(),
        java_version: java.version,
        max_memory_mb: max_mb,
        classpath_count: classpath.len(),
        missing_files: missing,
    })
}

/// 该版本要求的最低 Java 主版本（用于自动挑 Java）。
pub async fn required_java(version_id: &str, cfg: &Config) -> u32 {
    let Ok(chain) = manifest::version_chain(version_id, cfg).await else {
        return 8;
    };
    let merged = manifest::merge_chain(&chain);
    merged
        .get("javaVersion")
        .and_then(|v| v.get("majorVersion"))
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .unwrap_or(8)
}
