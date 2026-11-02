//! GlassMC — 液态玻璃 Minecraft 启动器（Tauri 2 版，macOS / Windows / Linux）

mod account;
mod commands;
mod commands_ext;
mod config;
mod download;
mod game;
mod glass;
mod importer;
mod injector;
mod install;
mod java;
mod javadl;
mod launch;
mod loader;
mod manifest;
mod microsoft;
mod modpack;
mod modrinth;
mod net;
mod paths;
mod skins;
mod terracotta;
mod yggdrasil;

use tauri::Manager;

/// 调试 / 导出用：构建某个版本的启动命令并返回可读文本。
/// 不加任何 UI，纯命令行，便于和原生版逐项对比参数。
pub fn describe_command(version_id: &str) -> Result<String, String> {
    let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    rt.block_on(async {
        let cfg = config::load();
        let acc = commands::resolve_account(&cfg);
        let required = launch::required_java(version_id, &cfg).await;
        let java = java::best_for(required, &cfg.java_path)
            .ok_or_else(|| "没有找到可用的 Java".to_string())?;
        let built = launch::build_command(version_id, &acc, &java, &cfg).await?;

        let mut out = String::new();
        out.push_str(&format!("版本      : {}\n", built.version_id));
        out.push_str(&format!("账号      : {} ({})\n", acc.username, acc.kind));
        out.push_str(&format!("Java      : {} (major {})\n", built.program, built.java_version));
        out.push_str(&format!("工作目录  : {}\n", built.cwd));
        out.push_str(&format!("内存      : {} MB\n", built.max_memory_mb));
        out.push_str(&format!("classpath : {} 项，缺失 {} 项\n", built.classpath_count, built.missing_files));
        let leftover: Vec<&String> = built.args.iter().filter(|a| a.contains("${")).collect();
        out.push_str(&format!("未替换占位符: {}\n", if leftover.is_empty() { "无".to_string() } else { format!("{:?}", leftover) }));
        out.push_str("\n命令:\n");
        out.push_str(&format!("  {}\n", built.program));
        for a in &built.args {
            out.push_str(&format!("  {}\n", a));
        }
        Ok(out)
    })
}

/// 命令行安装（调试 / 批量部署用）：`glassmc --install 1.21.4`
pub fn cli_install(version_id: &str) -> Result<String, String> {
    let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    rt.block_on(async {
        let cfg = config::load();
        let outcome = install::install_version(None, version_id, &cfg).await?;
        let mut out = String::new();
        out.push_str(&format!("版本      : {}\n", outcome.version_id));
        out.push_str(&format!("下载      : {} 个文件\n", outcome.downloaded));
        out.push_str(&format!("已存在    : {} 个文件\n", outcome.skipped));
        out.push_str(&format!("资源对象  : {} 个\n", outcome.assets));
        out.push_str(&format!("失败      : {}\n", outcome.failed.len()));
        for f in outcome.failed.iter().take(20) {
            out.push_str(&format!("  - {f}\n"));
        }
        Ok(out)
    })
}

/// 命令行自测联机组件：下载 → 拉起守护进程 → 读 /state → 关闭。
pub fn cli_test_terracotta() -> Result<String, String> {
    let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    rt.block_on(async {
        let mut out = String::new();
        let snap = terracotta::bootstrap().await?;
        out.push_str(&format!("组件      : {}\n", snap.version));
        out.push_str(&format!("easytier  : {}\n", snap.easytier));
        out.push_str(&format!("端口      : {:?}\n", snap.port));
        match terracotta::refresh().await {
            Ok(v) => out.push_str(&format!("状态      : {v}\n")),
            Err(e) => out.push_str(&format!("状态读取失败：{e}\n")),
        }
        terracotta::shutdown();
        out.push_str("已请求关闭组件\n");
        Ok(out)
    })
}

pub fn run() {
    paths::ensure_dirs();
    let initial = config::load();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(config::ConfigState(std::sync::Mutex::new(initial)))
        .setup(|app| {
            if let Some(win) = app.get_webview_window("main") {
                let tier = glass::apply(&win);
                println!("[glass] 窗口玻璃档位 = {tier}");
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::get_config,
            commands::save_config,
            commands::scan_versions,
            commands::list_remote,
            commands::list_java,
            commands::preview_command,
            commands::launch_game,
            commands::kill_game,
            commands::game_running,
            commands::open_path,
            commands::open_url,
            commands::window_action,
            commands::set_opacity,
            commands::account_list,
            commands::set_active_account,
            commands::create_offline_account,
            commands::list_mods,
            // ---- 安装 ----
            commands_ext::install_version,
            commands_ext::version_detail,
            commands_ext::delete_version,
            commands_ext::loader_versions,
            commands_ext::install_loader,
            // ---- Java ----
            commands_ext::java_candidates,
            commands_ext::install_java,
            commands_ext::remove_java,
            commands_ext::list_java_all,
            // ---- 账号 ----
            commands_ext::accounts_all,
            commands_ext::remove_account,
            commands_ext::ygg_login,
            commands_ext::add_auth_server,
            commands_ext::remove_auth_server,
            commands_ext::ms_start,
            commands_ext::ms_poll,
            commands_ext::refresh_account,
            commands_ext::sync_account_skin,
            commands_ext::account_skin,
            commands_ext::refresh_account_skin,
            // ---- 皮肤库 ----
            commands_ext::skinlib_page,
            commands_ext::skinlib_apply,
            commands_ext::skinlib_clear,
            commands_ext::skinlib_thumb,
            // ---- 注入器 ----
            commands_ext::injector_status,
            commands_ext::injector_install,
            // ---- 模组 ----
            commands_ext::mods_scan,
            commands_ext::mods_toggle,
            commands_ext::mods_delete,
            commands_ext::mods_dir,
            commands_ext::modrinth_search,
            commands_ext::modrinth_versions,
            commands_ext::modrinth_install,
            // ---- 导入 ----
            commands_ext::import_detect,
            commands_ext::import_run,
            commands_ext::modpack_list,
            commands_ext::modpack_inspect,
            commands_ext::modpack_import,
            commands_ext::modpack_export,
            // ---- 联机 ----
            commands_ext::tc_snapshot,
            commands_ext::tc_bootstrap,
            commands_ext::tc_refresh,
            commands_ext::tc_create,
            commands_ext::tc_join,
            commands_ext::tc_leave,
            commands_ext::tc_shutdown,
            // ---- 工具 ----
            commands_ext::pick_file,
            commands_ext::pick_save_path,
            commands_ext::pick_folder,
            commands_ext::open_dir,
            commands_ext::data_root,
            commands_ext::version_dir,
            commands_ext::app_version,
        ])
        .build(tauri::generate_context!())
        .expect("构建 GlassMC 失败")
        .run(|_app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                let _ = game::kill();
                terracotta::kill_on_exit();
            }
        });
}
