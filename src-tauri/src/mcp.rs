//! GlassMC 内置 MCP 服务端（stdio 传输）。
//!
//! 启动方式：`glassmc --mcp`。MCP 客户端把它当作子进程拉起，通过 stdin 写
//! 换行分隔的 JSON-RPC 2.0 请求，stdout 读响应。所有日志只走 stderr，
//! 绝不污染协议通道。
//!
//! 只暴露「读状态 + 用户明确调用才启动/备份」的工具；不提供删除、覆盖、
//! 执行任意脚本等破坏性能力。

use serde::Serialize;
use serde_json::{json, Value};
use std::io::{BufRead, Write};

use crate::{autoupdate, commands, config, diagnostics, mods_advanced, saves, servers};

const PROTOCOL: &str = "2024-11-05";
/// 我们实现的工具能力在这几个协议版本下都没有差异，客户端用哪个就回显哪个。
const SUPPORTED_PROTOCOLS: [&str; 3] = ["2024-11-05", "2025-03-26", "2025-06-18"];

/// 给 MCP 客户端展示的工具清单。
fn tools() -> Value {
    json!([
        {
            "name": "list_versions",
            "description": "列出本机已安装的 Minecraft 实例（id、加载器、继承关系、模组数）。",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "get_config",
            "description": "读取 GlassMC 当前设置（内存、Java、语言、下载源等），只读。",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "list_mods",
            "description": "列出某个实例已安装的模组文件及启用状态。",
            "inputSchema": {
                "type": "object",
                "properties": { "versionId": { "type": "string", "description": "实例 id" } },
                "required": ["versionId"]
            }
        },
        {
            "name": "list_saves",
            "description": "列出某个实例的世界存档（名称、种子、估算时长、最后游玩时间）。",
            "inputSchema": {
                "type": "object",
                "properties": { "versionId": { "type": "string" } },
                "required": ["versionId"]
            }
        },
        {
            "name": "backup_save",
            "description": "备份一个世界存档。会复制到独立的备份目录，不改动原存档。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "versionId": { "type": "string" },
                    "saveId": { "type": "string" },
                    "label": { "type": "string", "description": "可选备份备注" }
                },
                "required": ["versionId", "saveId"]
            }
        },
        {
            "name": "crash_explain",
            "description": "读取最近的崩溃/启动日志并给出通俗解释和排查建议。",
            "inputSchema": {
                "type": "object",
                "properties": { "versionId": { "type": "string" } },
                "required": ["versionId"]
            }
        },
        {
            "name": "cleanup_scan",
            "description": "扫描可清理的冗余文件，只返回清单，不执行删除。",
            "inputSchema": {
                "type": "object",
                "properties": { "versionId": { "type": "string" } },
                "required": ["versionId"]
            }
        },
        {
            "name": "mod_conflicts",
            "description": "检测某个实例的模组冲突（重复 modid、多版本、重复文件）。",
            "inputSchema": {
                "type": "object",
                "properties": { "versionId": { "type": "string" } },
                "required": ["versionId"]
            }
        },
        {
            "name": "server_ping",
            "description": "测试一个 Java 版服务器的延迟、人数与 MOTD。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "address": { "type": "string" },
                    "port": { "type": "integer", "default": 25565 }
                },
                "required": ["address"]
            }
        },
        {
            "name": "frp_status",
            "description": "查看 FRP 客户端的运行状态与二进制安装情况。",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "auto_update_status",
            "description": "查看静默自动更新的配置和已暂存的安装包。",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "launch",
            "description": "启动指定实例（等同于桌面快捷方式启动）。调用后返回进程信息。",
            "inputSchema": {
                "type": "object",
                "properties": { "versionId": { "type": "string" } },
                "required": ["versionId"]
            }
        }
    ])
}

fn arg_str(args: &Value, key: &str) -> Result<String, String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| format!("缺少参数 {key}"))
}

fn pretty<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string_pretty(value).map_err(|e| e.to_string())
}

fn call_tool(name: &str, args: &Value, rt: &tokio::runtime::Runtime) -> Result<String, String> {
    match name {
        "list_versions" => pretty(&commands::scan_versions()),
        "get_config" => pretty(&config::load()),
        "list_mods" => pretty(&commands::list_mods(arg_str(args, "versionId")?)),
        "list_saves" => pretty(&saves::saves_list(arg_str(args, "versionId")?)),
        "backup_save" => {
            let version = arg_str(args, "versionId")?;
            let save = arg_str(args, "saveId")?;
            let label = args
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or("mcp")
                .to_string();
            pretty(&saves::saves_backup(version, save, label)?)
        }
        "crash_explain" => pretty(&diagnostics::crash_explain(arg_str(args, "versionId")?)?),
        "cleanup_scan" => pretty(&diagnostics::cleanup_scan(arg_str(args, "versionId")?)?),
        "mod_conflicts" => pretty(&mods_advanced::mods_conflicts(arg_str(args, "versionId")?)),
        "server_ping" => {
            let address = arg_str(args, "address")?.trim().to_string();
            if address.len() > 255 {
                return Err("address 过长（最多 255 字符）".into());
            }
            let port = match args.get("port") {
                None | Some(Value::Null) => 25565,
                Some(v) => v
                    .as_u64()
                    .filter(|p| (1..=65535).contains(p))
                    .ok_or("port 必须是 1-65535 的整数")? as u16,
            };
            pretty(&rt.block_on(servers::servers_ping(address, port)))
        }
        "frp_status" => pretty(&servers::frp_status()),
        "auto_update_status" => pretty(&autoupdate::auto_status()),
        "launch" => crate::cli_launch(&arg_str(args, "versionId")?),
        other => Err(format!("未知工具：{other}")),
    }
}

/// 处理一条 JSON-RPC 消息；通知类消息不返回内容。
fn handle_line(line: &str, rt: &tokio::runtime::Runtime) -> Option<String> {
    let req: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            return Some(
                json!({
                    "jsonrpc": "2.0",
                    "id": Value::Null,
                    "error": { "code": -32700, "message": format!("JSON 解析失败：{e}") }
                })
                .to_string(),
            )
        }
    };
    // 根节点必须是对象；数组/数字等直接按 Invalid Request 处理，避免客户端空等
    if !req.is_object() {
        return Some(
            json!({
                "jsonrpc": "2.0",
                "id": Value::Null,
                "error": { "code": -32600, "message": "请求必须是 JSON 对象" }
            })
            .to_string(),
        );
    }
    // 没有 id 的是通知（例如 notifications/initialized）或客户端发来的响应，按规范不回复
    let id = req.get("id").cloned()?;
    // 有 id 但 method 不是字符串：这是坏请求，必须回错误而不是静默丢弃
    let method = match req.get("method").and_then(Value::as_str) {
        Some(m) => m,
        None => {
            return Some(
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": { "code": -32600, "message": "缺少字符串类型的 method" }
                })
                .to_string(),
            )
        }
    };

    let result: Result<Value, (i64, String)> = match method {
        "initialize" => {
            // 客户端（Claude Desktop 等）可能用较新的协议版本。我们的能力集合在这些
            // 版本间没有变化，回显客户端请求的版本可避免对方判定协商失败。
            let requested = req
                .get("params")
                .and_then(|p| p.get("protocolVersion"))
                .and_then(Value::as_str)
                .unwrap_or(PROTOCOL);
            let version = if SUPPORTED_PROTOCOLS.contains(&requested) {
                requested
            } else {
                PROTOCOL
            };
            Ok(json!({
                "protocolVersion": version,
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": "glassmc", "version": env!("CARGO_PKG_VERSION") },
                "instructions": "GlassMC 启动器工具。修改类操作（备份、启动）会在本机执行，删除类操作只扫描不执行。"
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools() })),
        "tools/call" => {
            let params = req.get("params").cloned().unwrap_or_else(|| json!({}));
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                call_tool(&name, &args, rt)
            }));
            let text =
                match outcome {
                    Ok(Ok(text)) => text,
                    Ok(Err(e)) => {
                        return Some(
                            json!({
                                "jsonrpc": "2.0", "id": id,
                                "result": {
                                    "content": [{ "type": "text", "text": e }],
                                    "isError": true
                                }
                            })
                            .to_string(),
                        )
                    }
                    Err(_) => return Some(
                        json!({
                            "jsonrpc": "2.0", "id": id,
                            "result": {
                                "content": [{ "type": "text", "text": "工具执行时发生内部错误" }],
                                "isError": true
                            }
                        })
                        .to_string(),
                    ),
                };
            Ok(json!({
                "content": [{ "type": "text", "text": text }],
                "isError": false
            }))
        }
        "resources/list" => Ok(json!({ "resources": [] })),
        "prompts/list" => Ok(json!({ "prompts": [] })),
        other => Err((-32601, format!("不支持的方法：{other}"))),
    };

    let response = match result {
        Ok(value) => json!({ "jsonrpc": "2.0", "id": id, "result": value }),
        Err((code, message)) => json!({
            "jsonrpc": "2.0", "id": id,
            "error": { "code": code, "message": message }
        }),
    };
    Some(response.to_string())
}

/// stdio 主循环。由 `--mcp` 触发。
pub fn serve() -> Result<(), String> {
    crate::paths::ensure_dirs();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(2)
        .build()
        .map_err(|e| e.to_string())?;
    eprintln!(
        "[GlassMC MCP] stdio 服务已启动，工具数 {}",
        tools().as_array().map_or(0, Vec::len)
    );

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[GlassMC MCP] 读取 stdin 失败：{e}");
                break;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        if let Some(response) = handle_line(&line, &rt) {
            writeln!(stdout, "{response}").map_err(|e| e.to_string())?;
            stdout.flush().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// 给设置页用的状态信息：让用户知道该把哪个命令写进 MCP 客户端配置。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpStatus {
    pub executable: String,
    pub args: Vec<String>,
    pub tool_count: usize,
    pub config_example: String,
}

#[tauri::command]
pub fn mcp_status() -> McpStatus {
    let executable = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "glassmc".to_string());
    let example = json!({
        "mcpServers": {
            "glassmc": { "command": executable, "args": ["--mcp"] }
        }
    })
    .to_string();
    McpStatus {
        executable: executable.clone(),
        args: vec!["--mcp".to_string()],
        tool_count: tools().as_array().map_or(0, Vec::len),
        config_example: example,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    #[test]
    fn initialize_and_tools_list() {
        let rt = rt();
        let init = handle_line(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
            &rt,
        )
        .unwrap();
        let v: Value = serde_json::from_str(&init).unwrap();
        assert_eq!(v["result"]["serverInfo"]["name"], "glassmc");
        assert_eq!(v["result"]["protocolVersion"], PROTOCOL);

        let modern = handle_line(
            r#"{"jsonrpc":"2.0","id":9,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
            &rt,
        )
        .unwrap();
        let v: Value = serde_json::from_str(&modern).unwrap();
        assert_eq!(v["result"]["protocolVersion"], "2025-06-18");

        let list = handle_line(
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#,
            &rt,
        )
        .unwrap();
        let v: Value = serde_json::from_str(&list).unwrap();
        assert!(v["result"]["tools"].as_array().unwrap().len() >= 10);
    }

    #[test]
    fn notifications_have_no_response() {
        let rt = rt();
        assert!(handle_line(
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            &rt
        )
        .is_none());
    }

    #[test]
    fn malformed_requests_get_32600() {
        let rt = rt();
        let bad_method = handle_line(r#"{"jsonrpc":"2.0","id":5,"method":123}"#, &rt).unwrap();
        let v: Value = serde_json::from_str(&bad_method).unwrap();
        assert_eq!(v["error"]["code"], -32600);
        assert_eq!(v["id"], 5);

        let bad_root = handle_line("[1,2,3]", &rt).unwrap();
        let v: Value = serde_json::from_str(&bad_root).unwrap();
        assert_eq!(v["error"]["code"], -32600);
        assert!(v["id"].is_null());
    }

    #[test]
    fn unknown_tool_is_reported_as_tool_error() {
        let rt = rt();
        let out = handle_line(
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"nope","arguments":{}}}"#,
            &rt,
        )
        .unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["result"]["isError"], true);
    }
}
