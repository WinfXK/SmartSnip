//! Copyright (c) 2026 Winfxk (冰月)
//! Email: admin@winfxk.cn  QQ: 2508543202
//! 创建时间: 2026年11月23日
//!
//! 文字标注输入框：在点击处弹出单行输入 + 确定/取消，确认后向遮罩投递 TEXT_COMMITTED。

use std::sync::Mutex;

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{GetStockObject, DEFAULT_GUI_FONT};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetSystemMetrics, GetWindowTextW, IsWindow,
    RegisterClassW, SendMessageW, SetForegroundWindow, ShowWindow, HMENU, SM_CXVIRTUALSCREEN,
    SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_SHOW, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_CLOSE, WM_COMMAND, WM_DESTROY, WM_SETFONT, WNDCLASSW,
};

const CLASS_NAME: &str = "SmartSnip_TextInputWindow";
const ID_EDIT: i32 = 2001;
const ID_OK: i32 = 2002;
const ID_CANCEL: i32 = 2003;

/// 待提交的文字标注（内容 + 选区局部坐标）
pub static PENDING: Mutex<Option<(String, i32, i32)>> = Mutex::new(None);

struct InputState {
    hwnd: HWND,
    owner: HWND,
    edit: HWND,
    local: (i32, i32),
}

static mut INPUT: Option<InputState> = None;

/// 在屏幕坐标 `screen_pos` 附近弹出输入框；`local` 为选区局部坐标
pub fn show(owner: HWND, screen_pos: (i32, i32), local: (i32, i32)) {
    unsafe {
        if let Some(st) = INPUT.as_ref() {
            if IsWindow(st.hwnd).as_bool() {
                let _ = DestroyWindow(st.hwnd);
            }
        }
        let Ok(hinstance) = GetModuleHandleW(None) else { return };
        let class_name = HSTRING::from(CLASS_NAME);
        static REGISTERED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        if REGISTERED.get().is_none() {
            let wc = WNDCLASSW {
                lpfnWndProc: Some(wnd_proc),
                hInstance: hinstance.into(),
                hIcon: crate::icon::app_icon(),
                lpszClassName: PCWSTR(class_name.as_ptr()),
                ..Default::default()
            };
            RegisterClassW(&wc);
            REGISTERED.set(()).ok();
        }

        let (w, h) = (320, 92);
        let sx = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let sy = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let sw = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let sh = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        let x = (screen_pos.0 + 8).min(sx + sw - w).max(sx);
        let y = (screen_pos.1 + 8).min(sy + sh - h).max(sy);

        let hwnd = match CreateWindowExW(
            WINDOW_EX_STYLE(0x0000_0008 | 0x0000_0080), // WS_EX_TOPMOST | WS_EX_TOOLWINDOW
            PCWSTR(class_name.as_ptr()),
            windows::core::w!("文字标注"),
            WINDOW_STYLE(0x8000_0000 | 0x1000_0000), // WS_POPUP | WS_VISIBLE
            x,
            y,
            w,
            h,
            None,
            None,
            HINSTANCE(hinstance.0),
            None,
        ) {
            Ok(hwnd) => hwnd,
            Err(err) => {
                crate::logging::error(&format!("文字输入框创建失败: {err}"));
                return;
            }
        };
        let edit = control(hwnd, "EDIT", "", 0x5000_0000 | 0x0080_0000 | 0x0000_0080, 10, 10, 300, 26, ID_EDIT, hinstance.into());
        control(hwnd, "BUTTON", "确定", 0x5000_0000 | 0x0000_0001, 120, 50, 90, 28, ID_OK, hinstance.into());
        control(hwnd, "BUTTON", "取消", 0x5000_0000 | 0x0000_0001, 220, 50, 90, 28, ID_CANCEL, hinstance.into());
        if !edit.is_invalid() {
            let font = GetStockObject(DEFAULT_GUI_FONT);
            let _ = SendMessageW(edit, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
        }
        INPUT = Some(InputState { hwnd, owner, edit, local });
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        if !edit.is_invalid() {
            let _ = SetFocus(edit);
        }
    }
}

unsafe fn control(
    parent: HWND,
    class: &str,
    text: &str,
    style: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    id: i32,
    hinst: HINSTANCE,
) -> HWND {
    let class = HSTRING::from(class);
    let text = HSTRING::from(text);
    CreateWindowExW(
        WINDOW_EX_STYLE(0),
        PCWSTR(class.as_ptr()),
        PCWSTR(text.as_ptr()),
        WINDOW_STYLE(style),
        x,
        y,
        w,
        h,
        parent,
        HMENU(id as isize as *mut core::ffi::c_void),
        hinst,
        None,
    )
    .unwrap_or_default()
}

fn get_text(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

unsafe fn confirm(hwnd: HWND) {
    let Some(st) = INPUT.as_ref() else { return };
    let text = get_text(st.edit);
    if !text.trim().is_empty() {
        *PENDING.lock().unwrap_or_else(|p| p.into_inner()) =
            Some((text, st.local.0, st.local.1));
        let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
            st.owner,
            crate::app::TEXT_COMMITTED,
            WPARAM(0),
            LPARAM(0),
        );
    }
    let _ = DestroyWindow(hwnd);
}

extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_COMMAND => {
                match (wparam.0 & 0xFFFF) as i32 {
                    ID_OK => confirm(hwnd),
                    ID_CANCEL => {
                        let _ = DestroyWindow(hwnd);
                    }
                    _ => {}
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                INPUT = None;
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
