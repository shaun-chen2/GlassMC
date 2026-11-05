//! 窗口玻璃。三端能力不同，这里只做「能申请就申请」，申请失败交给前端降级。
//!
//! - macOS：NSVisualEffectView（`hudWindow` + withinWindow），效果最接近原生版
//! - Windows：Win11 走 Acrylic（实时模糊窗口后面的桌面），老系统退 Mica，再退纯透明
//! - Linux：没有统一 API，只能靠合成器；这里返回 frosted 让前端用半透明底

use tauri::WebviewWindow;

/// 实际生效的档位，启动时写一次，`bootstrap` 读给前端看。
pub struct TierState(pub std::sync::Mutex<String>);

#[cfg(target_os = "windows")]
mod win {
    //! window-vibrancy 只管「背景材质」，边框、圆角、深浅色这三件事得自己来。
    //!
    //! 1. `DWMWA_USE_IMMERSIVE_DARK_MODE`：界面是深色的，但系统主题是浅色时，
    //!    Acrylic / Mica 会铺一层近乎白色的底 —— 看上去就是「窗口是白的，透不出桌面」。
    //! 2. `DWMWA_BORDER_COLOR`：无边框窗口为了能拖拽改大小保留了 `WS_THICKFRAME`，
    //!    DWM 会沿着窗口描一条 1px 的灰线。置 NONE 关掉，拖拽热区不受影响。
    //! 3. `DWMWA_WINDOW_CORNER_PREFERENCE`：圆角交给 DWM 裁。
    //!    前端**不要**再画一条 CSS 弧线：两条弧半径差一点（DPI 缩放、抗锯齿），
    //!    角上就会露出一条亮边。由系统裁，前端外壳保持直角，缝就不存在了。

    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tauri::WebviewWindow;

    /// DWM 是否接管了圆角裁剪。告诉前端要不要自己画圆角。
    pub static ROUNDED: AtomicBool = AtomicBool::new(false);

    #[link(name = "dwmapi")]
    extern "system" {
        fn DwmSetWindowAttribute(
            hwnd: *mut c_void,
            attr: u32,
            value: *const c_void,
            size: u32,
        ) -> i32;
    }

    const USE_IMMERSIVE_DARK_MODE: u32 = 20;
    const WINDOW_CORNER_PREFERENCE: u32 = 33;
    const BORDER_COLOR: u32 = 34;

    const CORNER_ROUND: u32 = 2;
    const COLOR_NONE: u32 = 0xFFFF_FFFE;

    /// 设一个 DWM 属性。系统不认这个属性（老版本）时调用会失败，返回 false。
    unsafe fn set(hwnd: *mut c_void, attr: u32, value: u32) -> bool {
        DwmSetWindowAttribute(
            hwnd,
            attr,
            &value as *const u32 as *const c_void,
            std::mem::size_of::<u32>() as u32,
        ) == 0
    }

    /// 修整窗口本身的观感：深色材质、无描边、圆角交给 DWM。
    /// 老系统不认的属性会静默失败，不影响其余设置。
    pub fn prepare(window: &WebviewWindow) {
        let Ok(hwnd) = window.hwnd() else {
            return;
        };
        let hwnd = hwnd.0;
        let rounded = unsafe {
            set(hwnd, USE_IMMERSIVE_DARK_MODE, 1);
            set(hwnd, BORDER_COLOR, COLOR_NONE);
            set(hwnd, WINDOW_CORNER_PREFERENCE, CORNER_ROUND)
        };
        ROUNDED.store(rounded, Ordering::Relaxed);
    }
}

/// 圆角是不是由系统裁的。Windows 10 及更早不认这个属性，返回 false，前端自己画。
pub fn dwm_rounded() -> bool {
    #[cfg(target_os = "windows")]
    {
        win::ROUNDED.load(std::sync::atomic::Ordering::Relaxed)
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

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
            Ok(()) => "liquid".into(),
            Err(_) => "frosted".into(),
        }
    }

    #[cfg(target_os = "windows")]
    {
        use window_vibrancy::{apply_acrylic, apply_mica};

        // 先修窗口本身（深色、无描边、DWM 圆角），再谈背景材质
        win::prepare(window);

        // Acrylic 会实时模糊窗口后面的桌面，这才是「液态玻璃」；
        // Mica 只把壁纸调暗铺满，桌面上有别的东西挡着就基本看不出来。
        if apply_acrylic(window, Some((16, 24, 38, 140))).is_ok() {
            return "liquid".into();
        }
        if apply_mica(window, Some(true)).is_ok() {
            return "liquid".into();
        }
        "frosted".into()
    }

    #[cfg(target_os = "linux")]
    {
        let _ = window;
        "frosted".into()
    }
}
