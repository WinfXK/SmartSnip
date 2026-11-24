//! Copyright (c) 2026 Winfxk (冰月)
//! Email: admin@winfxk.cn  QQ: 2508543202
//! 创建时间: 2026年11月23日
//!
//! 应用图标：从 exe 内嵌资源（build.rs 注入，资源组 ID 1）加载。

use windows::core::PCWSTR;
use windows::Win32::Foundation::HINSTANCE;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    LoadImageW, HICON, IMAGE_ICON, LR_DEFAULTSIZE, LR_SHARED,
};

/// 图标资源组 ID（与 build.rs 写入的第一个图标组一致）
pub const APP_ICON_ID: u16 = 1;

/// 加载进程内嵌应用图标；失败返回空句柄
pub fn app_icon() -> HICON {
    unsafe {
        let Ok(module) = GetModuleHandleW(None) else {
            return HICON::default();
        };
        let name = PCWSTR::from_raw(APP_ICON_ID as usize as *const u16);
        LoadImageW(
            HINSTANCE(module.0),
            name,
            IMAGE_ICON,
            0,
            0,
            LR_DEFAULTSIZE | LR_SHARED,
        )
        .map(|handle| HICON(handle.0))
        .unwrap_or_default()
    }
}
