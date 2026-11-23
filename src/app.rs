//! Copyright (c) 2026 Winfxk (冰月)
//! Email: admin@winfxk.cn  QQ: 2508543202
//! 创建时间: 2026年11月23日
//!
//! 应用入口：DPI 感知声明 + 主消息窗口 + 消息循环。
//! 全程事件驱动（GetMessage 阻塞），无任何轮询。

use std::sync::OnceLock;
use windows::core::Result;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::SetProcessDpiAwarenessContext;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassW,
    TranslateMessage, CW_USEDEFAULT, DestroyWindow, MSG, WINDOW_EX_STYLE, WINDOW_STYLE,
    WM_DESTROY, WNDCLASSW,
};

/// 主窗口类名，进程内唯一
pub const MAIN_WINDOW_CLASS: &str = "SmartSnip_MainWindow";
/// 自定义消息基址，各模块通过 `WM_APP + n` 通信
pub const WM_APP_BASE: u32 = 0x8000;

static MAIN_HWND: OnceLock<isize> = OnceLock::new();

/// 主窗口句柄；模块发消息时使用
pub fn main_hwnd() -> HWND {
    HWND(MAIN_HWND.get().copied().unwrap_or_default() as *mut _)
}

/// per-monitor v2 感知上下文（Win32 登记值）
const DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2: DPI_AWARENESS_CONTEXT =
    DPI_AWARENESS_CONTEXT(-4isize as *mut _);

extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_DESTROY => {
            unsafe { windows::Win32::UI::WindowsAndMessaging::PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn set_process_dpi_awareness() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

pub fn run() -> Result<()> {
    set_process_dpi_awareness();
    unsafe {
        let hinstance = GetModuleHandleW(None)?;
        let class_name = windows::core::HSTRING::from(MAIN_WINDOW_CLASS);
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance.into(),
            lpszClassName: windows::core::PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        RegisterClassW(&wc);

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            windows::core::PCWSTR(class_name.as_ptr()),
            windows::core::w!("SmartSnip"),
            WINDOW_STYLE(0),
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            None,
            None,
            windows::Win32::Foundation::HINSTANCE(hinstance.0),
            None,
        )?;
        let _ = MAIN_HWND.set(hwnd.0 as isize);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let _ = DestroyWindow(hwnd);
    }
    Ok(())
}
