//! 窗口玻璃。三端能力不同，这里只做「能申请就申请」，申请失败交给前端降级。
//!
//! - macOS：NSVisualEffectView（`hudWindow` + withinWindow），效果最接近原生版
//! - Windows：优先 Mica（Win11），失败退 Acrylic（Win10 1803+）
//! - Linux：没有统一 API，只能靠合成器；这里返回 frosted 让前端用半透明底

use tauri::WebviewWindow;

/// 返回实际生效的档位：liquid / frosted
pub fn apply(window: &WebviewWindow) -> String {
    #[cfg(target_os = "macos")]
    {
        use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial, NSVisualEffectState};
        match apply_vibrancy(
            window,
            NSVisualEffectMaterial::HudWindow,
            Some(NSVisualEffectState::Active),
            Some(14.0),
        ) {
            Ok(()) => return "liquid".into(),
            Err(_) => return "frosted".into(),
        }
    }

    #[cfg(target_os = "windows")]
    {
        use window_vibrancy::{apply_acrylic, apply_mica};
        if apply_mica(window, None).is_ok() {
            return "liquid".into();
        }
        if apply_acrylic(window, Some((16, 24, 38, 170))).is_ok() {
            return "liquid".into();
        }
        return "frosted".into();
    }

    #[cfg(target_os = "linux")]
    {
        let _ = window;
        "frosted".into()
    }
}
