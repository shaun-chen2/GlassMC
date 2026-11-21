//! 实例运维：复制实例、分类 / 归档、桌面快捷方式。
//!
//! 所有操作都只碰 `versions/<id>/`，不改共享的 libraries / assets；
//! 「归档」只是把 id 记进配置，文件原地不动，随时可以恢复。

use std::path::{Path, PathBuf};

use tauri::AppHandle;

use crate::config::{self, Config, ConfigState};
use crate::install::Progress;
use crate::modpack;
use crate::paths;

/// 复制时默认跳过的目录：日志、崩溃报告、缓存、截图都可以重新生成，
/// 一个几百 MB 的整合包实例里这些经常占掉一半体积。
const SKIP_DIRS: &[&str] = &[
    "logs",
    "crash-reports",
    "screenshots",
    ".cache",
    ".mixin.out",
    ".fabric",
];

/// 复制一个实例，返回新实例 id。
pub fn copy_instance(
    app: Option<AppHandle>,
    src_id: &str,
    new_name: &str,
    with_mods: bool,
    with_saves: bool,
) -> Result<String, String> {
    let src_id = src_id.trim();
    if src_id.is_empty() {
        return Err("实例 id 不能为空".into());
    }
    let src = paths::paths().versions_dir.join(src_id);
    if !src.is_dir() {
        return Err(format!("找不到实例目录：{}", src.display()));
    }

    let base = if new_name.trim().is_empty() {
        format!("{src_id}-副本")
    } else {
        new_name.trim().to_string()
    };
    let dest_id = modpack::unique_instance_id(&base);
    let dest = paths::paths().versions_dir.join(&dest_id);

    let mut files: Vec<PathBuf> = Vec::new();
    collect_files(&src, Path::new(""), with_mods, with_saves, &mut files)?;
    if files.is_empty() {
        return Err("这个实例里没有可复制的文件".into());
    }

    let prog = Progress::new(app, "instance:copy");
    prog.stage("复制实例", files.len(), 0.0, 1.0);
    std::fs::create_dir_all(&dest).map_err(|e| format!("创建实例目录失败：{e}"))?;

    let mut copied: u64 = 0;
    for rel in &files {
        let target = dest.join(rel);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败：{e}"))?;
        }
        std::fs::copy(src.join(rel), &target)
            .map_err(|e| format!("复制 {} 失败：{e}", rel.display()))?;
        copied += 1;
        prog.tick(false);
    }
    prog.tick(true);
    prog.note(&format!("已复制 {copied} 个文件"));

    // 整合包记录里的 instanceId / name 要跟着改，否则「导入列表」会指回原实例
    let manifest = dest.join(modpack::MANIFEST);
    if let Ok(text) = std::fs::read_to_string(&manifest) {
        if let Ok(mut rec) = serde_json::from_str::<modpack::PackRecord>(&text) {
            rec.instance_id = dest_id.clone();
            rec.name = base.clone();
            if let Ok(s) = serde_json::to_string_pretty(&rec) {
                let _ = std::fs::write(&manifest, s);
            }
        }
    }

    Ok(dest_id)
}

fn collect_files(
    dir: &Path,
    rel: &Path,
    with_mods: bool,
    with_saves: bool,
    out: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in rd.filter_map(|e| e.ok()) {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        let path = entry.path();
        // 不跟随符号链接：既防目录环，也避免把实例外的数据复制进来
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            continue;
        }
        let child_rel = rel.join(&name);
        if meta.is_dir() {
            let top = rel.as_os_str().is_empty();
            if top && SKIP_DIRS.contains(&name_str.as_ref()) {
                continue;
            }
            if top && name_str == "mods" && !with_mods {
                continue;
            }
            if top && name_str == "saves" && !with_saves {
                continue;
            }
            collect_files(&path, &child_rel, with_mods, with_saves, out)?;
        } else {
            out.push(child_rel);
        }
    }
    Ok(())
}

/// 设置实例分类；传空字符串表示取消分类。
pub fn set_category(
    state: &ConfigState,
    version_id: &str,
    category: &str,
) -> Result<Config, String> {
    let key = version_id.trim();
    if key.is_empty() {
        return Err("实例 id 不能为空".into());
    }
    let mut guard = state.0.lock().unwrap();
    let cat = category.trim();
    if cat.is_empty() {
        guard.instance_categories.remove(key);
    } else {
        guard
            .instance_categories
            .insert(key.to_string(), cat.to_string());
    }
    let next = guard.clone();
    config::save(&next)?;
    Ok(next)
}

/// 归档 / 取消归档。归档只影响列表显示，文件仍然在 versions/ 里。
pub fn set_archived(
    state: &ConfigState,
    version_id: &str,
    archived: bool,
) -> Result<Config, String> {
    let key = version_id.trim();
    if key.is_empty() {
        return Err("实例 id 不能为空".into());
    }
    let mut guard = state.0.lock().unwrap();
    guard.archived_instances.retain(|x| x != key);
    if archived {
        guard.archived_instances.push(key.to_string());
        guard.archived_instances.sort();
    }
    let next = guard.clone();
    config::save(&next)?;
    Ok(next)
}

/// 在桌面创建「启动某个实例」的快捷方式，返回落盘路径。
pub fn create_shortcut(version_id: &str) -> Result<String, String> {
    let id = version_id.trim();
    if id.is_empty() {
        return Err("实例 id 不能为空".into());
    }
    let exe = std::env::current_exe().map_err(|e| format!("定位应用可执行文件失败：{e}"))?;
    let desktop = dirs::desktop_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join("Desktop")))
        .ok_or("找不到桌面目录")?;
    if !desktop.is_dir() {
        std::fs::create_dir_all(&desktop).map_err(|e| format!("创建桌面目录失败：{e}"))?;
    }
    let path = make_shortcut(&desktop, &exe, id)?;
    Ok(path.to_string_lossy().to_string())
}

fn shortcut_stem(version_id: &str) -> String {
    let cleaned: String = version_id
        .trim()
        .chars()
        .map(|c| if "/\\:*?\"<>|".contains(c) { '_' } else { c })
        .collect();
    format!("GlassMC - {cleaned}")
}

#[cfg(target_os = "macos")]
fn make_shortcut(desktop: &Path, exe: &Path, version_id: &str) -> Result<PathBuf, String> {
    use std::os::unix::fs::PermissionsExt;

    let bundle = desktop.join(format!("{}.app", shortcut_stem(version_id)));
    // 重新生成时先清掉旧的，避免残留文件让 LaunchServices 缓存错版本
    if bundle.exists() {
        std::fs::remove_dir_all(&bundle).map_err(|e| format!("覆盖旧快捷方式失败：{e}"))?;
    }
    let macos = bundle.join("Contents/MacOS");
    let resources = bundle.join("Contents/Resources");
    std::fs::create_dir_all(&macos).map_err(|e| format!("创建快捷方式失败：{e}"))?;
    std::fs::create_dir_all(&resources).map_err(|e| format!("创建快捷方式失败：{e}"))?;

    let name = xml_escape(&shortcut_stem(version_id));
    let bundle_id = format!("com.glassmc.launcher.shortcut.{}", sanitize_id(version_id));
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>{name}</string>
  <key>CFBundleDisplayName</key><string>{name}</string>
  <key>CFBundleIdentifier</key><string>{bundle_id}</string>
  <key>CFBundleExecutable</key><string>launch</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundleShortVersionString</key><string>1.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>LSBackgroundOnly</key><false/>
</dict>
</plist>
"#
    );
    std::fs::write(bundle.join("Contents/Info.plist"), plist)
        .map_err(|e| format!("写入 Info.plist 失败：{e}"))?;

    let script = format!(
        "#!/bin/sh\nexec \"{}\" --launch \"{}\"\n",
        exe.to_string_lossy().replace('"', "\\\""),
        version_id.replace('"', "\\\"")
    );
    let launcher = macos.join("launch");
    std::fs::write(&launcher, script).map_err(|e| format!("写入启动脚本失败：{e}"))?;
    std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| format!("设置脚本权限失败：{e}"))?;

    // 复用主应用的图标，快捷方式在桌面上才认得出来
    if let Some(contents) = exe.parent().and_then(|p| p.parent()) {
        let icon = contents.join("Resources/AppIcon.icns");
        if icon.is_file() {
            let _ = std::fs::copy(icon, resources.join("AppIcon.icns"));
        }
    }
    Ok(bundle)
}

#[cfg(target_os = "windows")]
fn make_shortcut(desktop: &Path, exe: &Path, version_id: &str) -> Result<PathBuf, String> {
    use base64::Engine;
    use std::os::windows::process::CommandExt;

    let target = desktop.join(format!("{}.lnk", shortcut_stem(version_id)));
    let script = format!(
        "$ws = New-Object -ComObject WScript.Shell; \
         $s = $ws.CreateShortcut('{}'); \
         $s.TargetPath = '{}'; \
         $s.Arguments = '--launch \"{}\"'; \
         $s.WorkingDirectory = '{}'; \
         $s.IconLocation = '{}'; \
         $s.Save()",
        ps_quote(&target.to_string_lossy()),
        ps_quote(&exe.to_string_lossy()),
        version_id.replace('\'', "''"),
        ps_quote(&exe.parent().unwrap_or(exe).to_string_lossy()),
        ps_quote(&exe.to_string_lossy()),
    );
    // -EncodedCommand 走 UTF-16LE，中文实例名不会被控制台代码页弄乱
    let utf16: Vec<u8> = script
        .encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .collect();
    let encoded = base64::engine::general_purpose::STANDARD.encode(utf16);
    let status = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-EncodedCommand",
            &encoded,
        ])
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .status()
        .map_err(|e| format!("调用 PowerShell 失败：{e}"))?;
    if !status.success() {
        return Err("创建快捷方式失败（PowerShell 返回非零）".into());
    }
    Ok(target)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn make_shortcut(desktop: &Path, exe: &Path, version_id: &str) -> Result<PathBuf, String> {
    use std::os::unix::fs::PermissionsExt;

    let target = desktop.join(format!("{}.desktop", shortcut_stem(version_id)));
    let body = format!(
        "[Desktop Entry]\nType=Application\nName={}\nExec=\"{}\" --launch \"{}\"\nTerminal=false\nCategories=Game;\n",
        shortcut_stem(version_id),
        exe.to_string_lossy().replace('"', "\\\""),
        version_id.replace('"', "\\\"")
    );
    std::fs::write(&target, body).map_err(|e| format!("写入 .desktop 失败：{e}"))?;
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| format!("设置权限失败：{e}"))?;
    Ok(target)
}

fn sanitize_id(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(target_os = "windows")]
fn ps_quote(s: &str) -> String {
    s.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use super::{sanitize_id, shortcut_stem, xml_escape};

    #[test]
    fn builds_safe_names() {
        assert_eq!(shortcut_stem("新蒸程1.7"), "GlassMC - 新蒸程1.7");
        assert_eq!(shortcut_stem("a/b:c"), "GlassMC - a_b_c");
        assert_eq!(sanitize_id("a/b-c"), "a_b_c");
        assert_eq!(xml_escape("a&b<c"), "a&amp;b&lt;c");
    }

    /// 复制规则：默认不带 mods、带 saves、跳过 logs。
    #[test]
    fn copies_with_filters() {
        let root = std::env::temp_dir().join(format!("glassmc-ops-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::env::set_var("GLASSMC_DATA", &root);
        let inst = root.join("minecraft/versions/demo");
        std::fs::create_dir_all(inst.join("mods")).unwrap();
        std::fs::create_dir_all(inst.join("saves/world")).unwrap();
        std::fs::create_dir_all(inst.join("logs")).unwrap();
        std::fs::write(inst.join("demo.json"), "{}").unwrap();
        std::fs::write(inst.join("mods/a.jar"), "jar").unwrap();
        std::fs::write(inst.join("saves/world/level.dat"), "nbt").unwrap();
        std::fs::write(inst.join("logs/game.log"), "log").unwrap();

        let id = super::copy_instance(None, "demo", "副本", false, true).unwrap();
        assert_eq!(id, "副本");
        let dest = root.join("minecraft/versions/副本");
        assert!(dest.join("demo.json").is_file());
        assert!(!dest.join("mods/a.jar").exists(), "没勾选时不该复制 mods");
        assert!(dest.join("saves/world/level.dat").is_file());
        assert!(!dest.join("logs/game.log").exists(), "日志不该复制");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn makes_macos_shortcut_bundle() {
        let root = std::env::temp_dir().join(format!("glassmc-sc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let exe = std::path::PathBuf::from("/Applications/GlassMC.app/Contents/MacOS/GlassMC");
        let bundle = super::make_shortcut(&root, &exe, "demo").unwrap();
        assert!(bundle.join("Contents/Info.plist").is_file());
        let script = std::fs::read_to_string(bundle.join("Contents/MacOS/launch")).unwrap();
        assert!(script.contains("--launch \"demo\""));
        assert!(script.contains("/Applications/GlassMC.app/Contents/MacOS/GlassMC"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
