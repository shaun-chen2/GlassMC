//! GlassMC — 液态玻璃 Minecraft 启动器（Tauri 2 版，macOS / Windows / Linux）

mod account;
mod autoupdate;
mod commands;
mod commands_ext;
mod community;
mod config;
mod curseforge;
mod diagnostics;
mod download;
mod game;
mod glass;
mod importer;
mod injector;
mod install;
mod instance;
mod java;
mod javadl;
mod launch;
mod loader;
mod manifest;
mod mcp;
mod microsoft;
mod modpack;
mod modpacks_advanced;
mod modrinth;
mod mods_advanced;
mod net;
mod paths;
mod quick;
mod saves;
mod security;
mod servers;
mod skins;
mod store;
mod terracotta;
mod tweaks;
mod update;
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
        out.push_str(&format!(
            "Java      : {} (major {})\n",
            built.program, built.java_version
        ));
        out.push_str(&format!("工作目录  : {}\n", built.cwd));
        out.push_str(&format!("内存      : {} MB\n", built.max_memory_mb));
        out.push_str(&format!(
            "classpath : {} 项，缺失 {} 项\n",
            built.classpath_count, built.missing_files
        ));
        let leftover: Vec<&String> = built.args.iter().filter(|a| a.contains("${")).collect();
        out.push_str(&format!(
            "未替换占位符: {}\n",
            if leftover.is_empty() {
                "无".to_string()
            } else {
                format!("{:?}", leftover)
            }
        ));
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

/// 命令行直接启动某个实例（桌面快捷方式用）：`glassmc --launch 新蒸程1.7`
pub fn cli_launch(version_id: &str) -> Result<String, String> {
    let cfg = config::load();
    let acc = commands::resolve_account(&cfg);
    let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    let built = rt.block_on(async {
        let required = launch::required_java(version_id, &cfg).await;
        let java = java::best_for(required, &cfg.java_path)
            .ok_or_else(|| "没有找到可用的 Java，请先在设置里指定一个".to_string())?;
        launch::build_command(version_id, &acc, &java, &cfg).await
    })?;

    // 快捷方式启动没有窗口，输出直接追加到同一个日志文件，出问题还能查
    let log_path = paths::paths().log_dir.join("game-latest.log");
    if let Some(parent) = log_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let out = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| format!("打开日志文件失败：{e}"))?;
    let err = out
        .try_clone()
        .map_err(|e| format!("复制日志句柄失败：{e}"))?;
    let mut cmd = std::process::Command::new(&built.program);
    cmd.args(&built.args)
        .current_dir(&built.cwd)
        .stdin(std::process::Stdio::null())
        .stdout(out)
        .stderr(err);
    let child = cmd.spawn().map_err(|e| format!("启动游戏失败：{e}"))?;
    Ok(format!("已启动 {}（PID {}）", version_id, child.id()))
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

/// 命令行入口：`glassmc --mcp`。stdio MCP 服务端，不打开界面。
pub fn run_mcp() -> Result<(), String> {
    mcp::serve()
}

pub fn run() {
    paths::ensure_dirs();
    let initial = config::load();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(config::ConfigState(std::sync::Mutex::new(initial)))
        .manage(glass::TierState(std::sync::Mutex::new(
            "frosted".to_string(),
        )))
        .setup(|app| {
            if let Some(win) = app.get_webview_window("main") {
                let tier = glass::apply(&win);
                println!("[glass] 窗口玻璃档位 = {tier}");
                if let Some(state) = app.try_state::<glass::TierState>() {
                    if let Ok(mut slot) = state.0.lock() {
                        *slot = tier;
                    }
                }
            }
            // 静默自动更新：命令内部会判断开关，用户未开启时不联网
            tauri::async_runtime::spawn(async {
                let _ = crate::autoupdate::auto_check_now().await;
            });
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
            commands_ext::modpack_probe,
            commands_ext::curseforge_import,
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
            // ---- 更新 ----
            commands_ext::update_check,
            commands_ext::update_download,
            // ---- 静默自动更新 ----
            autoupdate::auto_config_get,
            autoupdate::auto_config_set,
            autoupdate::auto_status,
            autoupdate::auto_check_now,
            autoupdate::auto_mark_installed,
            // ---- 安全 ----
            security::mods_security_scan,
            security::mc_news,
            security::mc_news_refresh,
            // ---- 整合包进阶 ----
            modpacks_advanced::pack_share_code,
            modpacks_advanced::pack_preview_code,
            modpacks_advanced::pack_install_code,
            modpacks_advanced::pack_diff,
            modpacks_advanced::pack_apply_diff,
            // ---- 快捷游玩 / Discord RPC ----
            quick::pins_list,
            quick::pin_add,
            quick::pin_remove,
            quick::quick_prepare,
            quick::rpc_config,
            quick::rpc_status,
            quick::rpc_send_activity,
            // ---- 模组进阶 ----
            mods_advanced::preset_list,
            mods_advanced::preset_save,
            mods_advanced::preset_apply,
            mods_advanced::preset_delete,
            mods_advanced::mods_conflicts,
            mods_advanced::mods_drop_import,
            mods_advanced::mods_update_check,
            mods_advanced::mods_update_apply,
            // ---- 存档与数据 ----
            saves::saves_list,
            saves::saves_backup,
            saves::saves_list_backups,
            saves::saves_restore,
            saves::saves_delete_backup,
            saves::screenshots_list,
            saves::screenshots_delete,
            saves::saves_datapacks,
            saves::saves_toggle_datapack,
            saves::saves_add_datapack,
            saves::schematic_list,
            saves::schematic_import,
            // ---- 联机 / 服务器 / FRP ----
            servers::servers_list,
            servers::servers_add,
            servers::servers_remove,
            servers::servers_pin,
            servers::servers_ping,
            servers::servers_ping_all,
            servers::servers_join,
            servers::server_browser_list,
            servers::server_browser_info,
            servers::frp_download,
            servers::frp_config_get,
            servers::frp_config_save,
            servers::frp_status,
            servers::frp_start,
            servers::frp_stop,
            servers::frp_log,
            // ---- 诊断 ----
            diagnostics::crash_explain,
            diagnostics::logs_export,
            diagnostics::logs_reveal,
            diagnostics::cleanup_scan,
            diagnostics::cleanup_apply,
            // ---- 社区 / 硬件 ----
            community::share_export,
            community::share_text,
            community::mcbbs_template_get,
            community::mcbbs_template_set,
            community::mcbbs_render,
            community::server_dashboard,
            community::portable_export,
            community::hardware_info,
            community::hardware_usage,
            // ---- 移动端等价 / 辅助包 / 启动优化 ----
            tweaks::tweak_prefs_get,
            tweaks::tweak_prefs_set,
            tweaks::low_memory_set,
            tweaks::low_memory_args,
            tweaks::memory_recommend,
            tweaks::data_dir_targets,
            tweaks::data_dir_move,
            tweaks::assist_pack_create,
            tweaks::assist_packs_list,
            tweaks::assist_pack_open,
            tweaks::fast_core_bench,
            tweaks::cleanup_processes_scan,
            tweaks::cleanup_processes_apply,
            tweaks::smart_gc_set,
            tweaks::smart_gc_args,
            tweaks::prelaunch_script_get,
            tweaks::prelaunch_script_set,
            tweaks::prelaunch_script_run,
            tweaks::memory_compression_args,
            // ---- 实例运维 ----
            commands_ext::copy_instance,
            commands_ext::set_instance_category,
            commands_ext::set_instance_archived,
            commands_ext::create_instance_shortcut,
            // ---- 界面资源 ----
            commands_ext::media_data_url,
            // ---- MCP ----
            mcp::mcp_status,
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

#[cfg(test)]
mod smoke_new_features {
    /// 在空数据/不存在版本上跑一遍新功能读取路径：只允许返回错误，不允许 panic。
    #[test]
    fn read_paths_are_panic_free() {
        let v = "no-such-version".to_string();
        let _ = crate::saves::saves_list(v.clone());
        let _ = crate::saves::saves_list_backups(v.clone());
        let _ = crate::saves::screenshots_list(v.clone());
        let _ = crate::saves::saves_datapacks(v.clone(), "world".into());
        let _ = crate::saves::schematic_list(v.clone());
        let _ = crate::mods_advanced::preset_list();
        let _ = crate::mods_advanced::mods_conflicts(v.clone());
        let _ = crate::modpacks_advanced::pack_diff("missing".into());
        let _ = crate::servers::servers_list();
        let _ = crate::servers::server_browser_list();
        let _ = crate::diagnostics::crash_explain(v.clone());
        let _ = crate::diagnostics::cleanup_scan(v.clone());
        let _ = crate::security::mods_security_scan(v.clone());
        let _ = crate::community::server_dashboard();
        let _ = crate::community::share_text(v.clone());
        let _ = crate::tweaks::tweak_prefs_get();
        let _ = crate::tweaks::fast_core_bench();
        let _ = crate::quick::pins_list();
        let _ = crate::quick::rpc_status();
        let _ = crate::autoupdate::auto_status();
    }
}
