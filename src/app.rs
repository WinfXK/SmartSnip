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
    WM_DESTROY, WM_HOTKEY, WM_CLOSE, WNDCLASSW,
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
/// 工具条动作分发（wparam = action id）
pub const TOOLBAR_ACTION: u32 = WM_APP_BASE + 5;
/// OCR 结果就绪（结果存于 OCR_PENDING）
pub const OCR_RESULT: u32 = WM_APP_BASE + 6;

/// OCR 后台任务结果中转（结果文本 / 错误信息）
pub static OCR_PENDING: std::sync::Mutex<Option<std::result::Result<Vec<crate::ocr::OcrLine>, String>>> =
    std::sync::Mutex::new(None);

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
    let cfg = crate::config::runtime();
    let combo = crate::hotkey::HotKeyCombo {
        modifiers: cfg.hotkey.modifiers.clone(),
        key: cfg.hotkey.key.clone(),
    };
    match crate::hotkey::RegisteredHotKey::register(hwnd, &combo) {
        Ok(hk) => {
            *HOTKEY_HANDLE.lock().unwrap_or_else(|p| p.into_inner()) = Some(hk);
            crate::logging::info("热键注册成功");
        }
        Err(err) => {
            crate::logging::error(&format!("热键注册失败({err})，请在设置中更换快捷键"));
            crate::tray::notify("SmartSnip", &format!("快捷键注册失败：{err}，请在设置中更换"));
        }
    }
}

static HOTKEY_HANDLE: std::sync::Mutex<Option<crate::hotkey::RegisteredHotKey>> =
    std::sync::Mutex::new(None);

/// 换绑全局截图热键；成功后新组合即时生效
pub fn rebind_hotkey(
    combo: &crate::hotkey::HotKeyCombo,
) -> std::result::Result<(), crate::hotkey::HotKeyError> {
    let mut guard = HOTKEY_HANDLE.lock().unwrap_or_else(|p| p.into_inner());
    match guard.as_mut() {
        Some(hk) => hk.rebind(combo),
        None => {
            let hk = crate::hotkey::RegisteredHotKey::register(main_hwnd(), combo)?;
            *guard = Some(hk);
            Ok(())
        }
    }
}

pub extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_DESTROY => {
            crate::tray::shutdown();
            unsafe { windows::Win32::UI::WindowsAndMessaging::PostQuitMessage(0) };
            LRESULT(0)
        }
        ACTIVATE_SNIP => {
            // 托盘单击 / 二次启动唤醒：触发一次截图
            if let Err(err) = crate::overlay::start() {
                crate::logging::error(&format!("覆盖层启动失败: {err}"));
            }
            LRESULT(0)
        }
        WM_HOTKEY => {
            if crate::hotkey::handle_wm_hotkey(wparam, lparam).is_some() {
                if let Err(err) = crate::overlay::start() {
                    crate::logging::error(&format!("覆盖层启动失败: {err}"));
                }
            }
            LRESULT(0)
        }
        SELECTION_DONE => {
            if let Some(rect) = crate::overlay::take_selection() {
                match crate::capture::capture_region(rect) {
                    Ok(frame) => {
                        if let Err(err) = crate::clipboard::copy_frame(&frame) {
                            crate::logging::error(&err);
                        } else {
                            crate::logging::info("已复制到剪贴板");
                        }
                    }
                    Err(err) => crate::logging::error(&format!("截图失败: {err}")),
                }
            }
            LRESULT(0)
        }
        WM_CLOSE => unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(hwnd);
            LRESULT(0)
        },
        _ => {
            if msg == OCR_RESULT {
                handle_ocr_result();
                return LRESULT(0);
            }
            if let Some(handled) = crate::tray::handle_message(hwnd, msg, wparam, lparam) {
                return handled;
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
    }
}

/// OCR 后台任务结果：全文写入剪贴板并弹窗展示（结果面板 UI 后续迭代）
fn handle_ocr_result() {
    let taken = OCR_PENDING.lock().unwrap_or_else(|p| p.into_inner()).take();
    let Some(result) = taken else { return };
    match result {
        Ok(lines) => {
            if lines.is_empty() {
                unsafe {
                    let text = windows::core::HSTRING::from(
                        "未识别到文字。\n\n如需识别其他语言，请在 设置 → 时间和语言 → 语言 中安装对应语言包",
                    );
                    let title = windows::core::HSTRING::from("文字识别");
                    let _ = windows::Win32::UI::WindowsAndMessaging::MessageBoxW(
                        main_hwnd(),
                        windows::core::PCWSTR(text.as_ptr()),
                        windows::core::PCWSTR(title.as_ptr()),
                        windows::Win32::UI::WindowsAndMessaging::MB_ICONINFORMATION
                            | windows::Win32::UI::WindowsAndMessaging::MB_TOPMOST,
                    );
                }
                return;
            }
            let text = lines
                .iter()
                .map(|l| l.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            let _ = crate::clipboard::copy_text(&text);
            crate::logging::info("OCR 完成，全文已入剪贴板");
            let rows = lines.iter().map(|l| l.text.clone()).collect::<Vec<_>>();
            let mut pt = windows::Win32::Foundation::POINT::default();
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut pt);
            }
            if let Err(err) = crate::ui::ocr_panel::show(rows, (pt.x, pt.y)) {
                crate::logging::error(&format!("OCR 结果面板创建失败: {err}"));
            }
        }
        Err(err) => crate::logging::error(&format!("OCR 失败: {err}")),
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
        let app_icon = crate::icon::app_icon();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance.into(),
            hIcon: app_icon,
            lpszClassName: windows::core::PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        RegisterClassW(&wc);

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            windows::core::PCWSTR(class_name.as_ptr()),
            windows::core::w!("SmartSnip"),
            WINDOW_STYLE(0x00C0_0000),
            0,
            0,
            200,
            100,
            None,
            None,
            windows::Win32::Foundation::HINSTANCE(hinstance.0),
            None,
        )
        ?;        let _ = MAIN_HWND.set(hwnd.0 as isize);
        let store = crate::config::ConfigStore::new(crate::config::ConfigStore::standard_dir());
        crate::config::set_runtime(store.load());
        if let Err(err) = crate::tray::init(hwnd) {
            crate::dbg_println!("托盘初始化失败: {err}");
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
