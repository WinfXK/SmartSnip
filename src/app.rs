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
    WM_DESTROY, WM_HOTKEY, WNDCLASSW,
};

/// 主窗口类名，进程内唯一
pub const MAIN_WINDOW_CLASS: &str = "SmartSnip_MainWindow";
/// 自定义消息基址，各模块通过 `WM_APP + n` 通信
pub const WM_APP_BASE: u32 = 0x8000;
/// 二次启动唤醒既有实例：触发一次截图
pub const ACTIVATE_SNIP: u32 = WM_APP_BASE + 1;
/// 托盘图标回调消息
pub const TRAY_CALLBACK: u32 = WM_APP_BASE + 2;
/// 截图热键的 WM_HOTKEY 注册 ID
pub const HOTKEY_ID_SNIP: u32 = 0xB001;
/// 覆盖层选区确认完成
pub const SELECTION_DONE: u32 = WM_APP_BASE + 3;
/// 一张贴图窗口关闭
pub const PIN_CLOSED: u32 = WM_APP_BASE + 4;

static MAIN_HWND: OnceLock<isize> = OnceLock::new();

/// 主窗口句柄；模块发消息时使用
pub fn main_hwnd() -> HWND {
    HWND(MAIN_HWND.get().copied().unwrap_or_default() as *mut _)
}

/// per-monitor v2 感知上下文（Win32 登记值）
const DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2: DPI_AWARENESS_CONTEXT =
    DPI_AWARENESS_CONTEXT(-4isize as *mut _);

/// 启动时按配置注册截图热键；注册进程生命周期持有
fn register_startup_hotkey(hwnd: HWND) {
    let store = crate::config::ConfigStore::new(crate::config::ConfigStore::standard_dir());
    let cfg = store.load();
    let combo = crate::hotkey::HotKeyCombo {
        modifiers: cfg.hotkey.modifiers.clone(),
        key: cfg.hotkey.key.clone(),
    };
    match crate::hotkey::RegisteredHotKey::register(hwnd, &combo) {
        Ok(hk) => {
            let _ = HOTKEY_HANDLE.set(hk);
        }
        Err(err) => eprintln!("热键注册失败({err})，请在设置中更换快捷键"),
    }
}

static HOTKEY_HANDLE: OnceLock<crate::hotkey::RegisteredHotKey> = OnceLock::new();

extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_DESTROY => {
            crate::tray::shutdown();
            unsafe { windows::Win32::UI::WindowsAndMessaging::PostQuitMessage(0) };
            LRESULT(0)
        }
        ACTIVATE_SNIP => {
            // 唤醒动作：Task 6 接入覆盖层后在此触发一次截图
            eprintln!("[{}] 唤醒既有实例：触发截图", crate::timefmt::now_cn_string());
            LRESULT(0)
        }
        WM_HOTKEY => {
            if crate::hotkey::handle_wm_hotkey(wparam, lparam).is_some() {
                let _ = crate::overlay::start();
            }
            LRESULT(0)
        }
        SELECTION_DONE => {
            if let Some(rect) = crate::overlay::take_selection() {
                match crate::capture::capture_region(rect) {
                    Ok(frame) => {
                        if let Err(err) = crate::clipboard::copy_frame(&frame) {
                            eprintln!("[{}] {err}", crate::timefmt::now_cn_string());
                        } else {
                            eprintln!("[{}] 已复制到剪贴板", crate::timefmt::now_cn_string());
                            // Task 9 接入：保留帧供贴图；Task 8 接入标注
                        }
                    }
                    Err(err) => eprintln!("[{}] 截图失败: {err}", crate::timefmt::now_cn_string()),
                }
            }
            LRESULT(0)
        }
        WM_CLOSE => unsafe {
            // 主窗口不可见，托盘「退出」即销毁；后续可见设置窗口单独处理
            let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(hwnd);
            LRESULT(0)
        },
        _ => {
            if let Some(handled) = crate::tray::handle_message(hwnd, msg, wparam, lparam) {
                return handled;
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
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
        if let Err(err) = crate::tray::init(hwnd) {
            eprintln!("托盘初始化失败: {err}");
        }
        register_startup_hotkey(hwnd);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let _ = DestroyWindow(hwnd);
    }
    Ok(())
}
