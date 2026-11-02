//! 游戏进程管理：启动、实时日志、退出通知、杀掉。
//!
//! 日志同时走两条路：一条 emit 给界面实时显示，一条落盘到 logs/game-latest.log，
//! 崩了之后还能翻。

use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::launch::BuiltCommand;
use crate::paths;

static CHILD: OnceLock<Mutex<Option<Child>>> = OnceLock::new();

fn slot() -> &'static Mutex<Option<Child>> {
    CHILD.get_or_init(|| Mutex::new(None))
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LogLine {
    stream: String,
    line: String,
}

pub fn is_running() -> bool {
    let mut guard = slot().lock().unwrap();
    match guard.as_mut() {
        Some(ch) => match ch.try_wait() {
            Ok(Some(_)) => {
                *guard = None;
                false
            }
            Ok(None) => true,
            Err(_) => {
                *guard = None;
                false
            }
        },
        None => false,
    }
}

fn pump<R: std::io::Read + Send + 'static>(
    app: AppHandle,
    reader: R,
    tag: &'static str,
    log_path: PathBuf,
) {
    std::thread::spawn(move || {
        let buf = BufReader::new(reader);
        for line in buf.lines() {
            let Ok(line) = line else { break };
            let _ = app.emit(
                "game-log",
                LogLine {
                    stream: tag.to_string(),
                    line: line.clone(),
                },
            );
            if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&log_path) {
                let _ = writeln!(f, "{line}");
            }
        }
    });
}

/// 启动游戏。返回 pid。
pub fn spawn(app: AppHandle, built: &BuiltCommand) -> Result<u32, String> {
    if is_running() {
        return Err("已经有一个游戏在运行了".into());
    }

    let mut cmd = Command::new(&built.program);
    cmd.args(&built.args)
        .current_dir(&built.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }

    let mut child = cmd.spawn().map_err(|e| {
        format!(
            "启动失败：{e}\n可执行文件：{}\n工作目录：{}",
            built.program, built.cwd
        )
    })?;
    let pid = child.id();

    let log_dir = paths::paths().log_dir.clone();
    let _ = std::fs::create_dir_all(&log_dir);
    let log_path = log_dir.join("game-latest.log");
    let header = format!(
        "=== GlassMC 启动 ===\n版本: {}\nJava: {} (major {})\n工作目录: {}\n内存: {} MB\nclasspath: {} 项，缺失 {} 项\n主命令: {}\n参数:\n  {}\n\n",
        built.version_id,
        built.program,
        built.java_version,
        built.cwd,
        built.max_memory_mb,
        built.classpath_count,
        built.missing_files,
        built.program,
        built.args.join("\n  ")
    );
    let _ = std::fs::write(&log_path, header);

    if let Some(out) = child.stdout.take() {
        pump(app.clone(), out, "out", log_path.clone());
    }
    if let Some(err) = child.stderr.take() {
        pump(app.clone(), err, "err", log_path);
    }

    *slot().lock().unwrap() = Some(child);

    // 监控退出
    let handle = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(350));
        let mut guard = slot().lock().unwrap();
        let Some(ch) = guard.as_mut() else { break };
        match ch.try_wait() {
            Ok(Some(status)) => {
                let code = status.code().unwrap_or(-1);
                *guard = None;
                drop(guard);
                let _ = handle.emit("game-exit", code);
                break;
            }
            Ok(None) => {}
            Err(_) => {
                *guard = None;
                break;
            }
        }
    });

    Ok(pid)
}

pub fn kill() -> Result<(), String> {
    let mut guard = slot().lock().unwrap();
    match guard.as_mut() {
        Some(ch) => {
            ch.kill().map_err(|e| format!("结束进程失败：{e}"))?;
            let _ = ch.wait();
            *guard = None;
            Ok(())
        }
        None => Err("没有正在运行的游戏".into()),
    }
}
