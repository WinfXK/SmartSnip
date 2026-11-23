//! Copyright (c) 2026 Winfxk (冰月)
//! Email: admin@winfxk.cn  QQ: 2508543202
//! 创建时间: 2026年11月23日
//!
//! 选区工具条：选区确认后弹出，提供 截图/贴图/文字识别/二维码 入口。
//! 原生绘制（无图片资源），悬停高亮，位置自动避让屏幕边缘。

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, EndPaint, FillRect, InvalidateRect, SelectObject,
    SetBkMode, SetTextColor, DrawTextW, CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY,
    DEFAULT_CHARSET, DT_CENTER, DT_SINGLELINE, DT_VCENTER, FF_DONTCARE, FW_NORMAL,
    OUT_DEFAULT_PRECIS, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetCursorPos, GetSystemMetrics, LoadCursorW,
    RegisterClassW, SetCursor, SetWindowPos, ShowWindow, HWND_TOPMOST, IDC_HAND, SM_CXSCREEN,
    SM_CYSCREEN, SW_SHOWNOACTIVATE, WINDOW_EX_STYLE, WINDOW_STYLE, WM_ERASEBKGND,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_PAINT, WM_SETCURSOR, WNDCLASSW,
};

use crate::capture::Rect;

const CLASS_NAME: &str = "SmartSnip_ToolbarWindow";

/// 工具动作 ID（经 WM_APP 消息回传覆盖层）
pub mod action {
    pub const SNIP: u32 = 1;
    pub const PIN: u32 = 2;
    pub const OCR: u32 = 3;
    pub const QR: u32 = 4;
}

const LABELS: [&str; 4] = ["截图", "贴图", "识文", "二维码"];
const BTN_W: i32 = 56;
const BTN_H: i32 = 34;
const PAD: i32 = 6;
const GAP: i32 = 4;

const BG_COLOR: COLORREF = COLORREF(0x0030322F); // 深灰工具条底
const BTN_COLOR: COLORREF = COLORREF(0x003D403C);
const HOVER_COLOR: COLORREF = COLORREF(0x005A5F58);
const TEXT_COLOR: COLORREF = COLORREF(0x00F0F0F0);

pub struct Toolbar {
    pub hwnd: HWND,
    /// 悬停按钮索引
    hover: i32,
}

static mut HOVER: i32 = -1;

fn toolbar_size() -> (i32, i32) {
    (PAD * 2 + BTN_W * LABELS.len() as i32 + GAP * (LABELS.len() as i32 - 1), BTN_H + PAD * 2)
}

/// 在选区右下角外侧弹出工具条；越界时翻转到选区内侧/屏幕内侧
pub fn show(overlay_hwnd: HWND, selection_desktop: Rect) -> Result<HWND, windows::core::Error> {
    unsafe {
        let hinstance = GetModuleHandleW(None)?;
        let class_name = windows::core::HSTRING::from(CLASS_NAME);
        static REGISTERED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        if REGISTERED.get().is_none() {
            let wc = WNDCLASSW {
                lpfnWndProc: Some(wnd_proc),
                hInstance: hinstance.into(),
                hCursor: LoadCursorW(None, IDC_HAND).unwrap_or_default(),
                lpszClassName: windows::core::PCWSTR(class_name.as_ptr()),
                ..Default::default()
            };
            RegisterClassW(&wc);
            REGISTERED.set(()).ok();
        }
        HOVER = -1;
        let (w, h) = toolbar_size();
        // 右下角外侧 → 向内翻 → 屏幕内
        let (sw, sh) = (
            GetSystemMetrics(SM_CXSCREEN),
            GetSystemMetrics(SM_CYSCREEN),
        );
        let mut x = selection_desktop.x + selection_desktop.width - w;
        let mut y = selection_desktop.y + selection_desktop.height + 6;
        if y + h > sh {
            y = (selection_desktop.y + selection_desktop.height - h - 6).max(0);
        }
        if x < 0 {
            x = 0;
        }
        if x + w > sw {
            x = sw - w;
        }
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0x0000_0008 | 0x0000_0080), // WS_EX_TOPMOST | WS_EX_TOOLWINDOW
            windows::core::PCWSTR(class_name.as_ptr()),
            windows::core::w!(""),
            WINDOW_STYLE(0x1000_0000), // WS_VISIBLE（避开 WS_POPUP 强制最大化怪癖）
            x,
            y,
            w,
            h,
            overlay_hwnd,
            None,
            windows::Win32::Foundation::HINSTANCE(hinstance.0),
            None,
        )?;
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        Ok(hwnd)
    }
}

pub fn destroy(hwnd: HWND) {
    unsafe {
        if !hwnd.is_invalid() {
            let _ = DestroyWindow(hwnd);
        }
    }
}

fn btn_rect(i: i32) -> RECT {
    RECT {
        left: PAD + i * (BTN_W + GAP),
        top: PAD,
        right: PAD + i * (BTN_W + GAP) + BTN_W,
        bottom: PAD + BTN_H,
    }
}

fn btn_from_point(x: i32, y: i32) -> i32 {
    for i in 0..LABELS.len() as i32 {
        let r = btn_rect(i);
        if x >= r.left && x < r.right && y >= r.top && y < r.bottom {
            return i;
        }
    }
    -1
}

fn action_for(i: i32) -> u32 {
    match i {
        0 => action::SNIP,
        1 => action::PIN,
        2 => action::OCR,
        _ => action::QR,
    }
}

unsafe fn make_font() -> windows::Win32::Graphics::Gdi::HFONT {
    let name = windows::core::HSTRING::from("Microsoft YaHei UI");
    CreateFontW(
        15,
        0,
        0,
        0,
        FW_NORMAL.0 as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET.0 as u32,
        OUT_DEFAULT_PRECIS.0 as u32,
        CLIP_DEFAULT_PRECIS.0 as u32,
        CLEARTYPE_QUALITY.0 as u32,
        FF_DONTCARE.0 as u32,
        windows::core::PCWSTR(name.as_ptr()),
    )
}

extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                let bg = CreateSolidBrush(BG_COLOR);
                FillRect(hdc, &ps.rcPaint, bg);
                let _ = windows::Win32::Graphics::Gdi::DeleteObject(
                    windows::Win32::Graphics::Gdi::HGDIOBJ(bg.0),
                );
                SetBkMode(hdc, TRANSPARENT);
                SetTextColor(hdc, TEXT_COLOR);
                let font = make_font();
                let old_font = SelectObject(hdc, windows::Win32::Graphics::Gdi::HGDIOBJ(font.0));
                for (i, label) in LABELS.iter().enumerate() {
                    let r = btn_rect(i as i32);
                    let fill = if HOVER == i as i32 { HOVER_COLOR } else { BTN_COLOR };
                    let brush = CreateSolidBrush(fill);
                    FillRect(hdc, &r, brush);
                    let _ = windows::Win32::Graphics::Gdi::DeleteObject(
                        windows::Win32::Graphics::Gdi::HGDIOBJ(brush.0),
                    );
                    let mut text: Vec<u16> = label.encode_utf16().collect();
                    text.push(0);
                    let mut rr = r;
                    let _ = DrawTextW(
                        hdc,
                        &mut text,
                        &mut rr,
                        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                    );
                }
                SelectObject(hdc, old_font);
                let _ = windows::Win32::Graphics::Gdi::DeleteObject(
                    windows::Win32::Graphics::Gdi::HGDIOBJ(font.0),
                );
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                let x = (lparam.0 & 0xFFFF) as u16 as i32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as u16 as i32;
                let hit = btn_from_point(x, y);
                if hit != HOVER {
                    HOVER = hit;
                    let _ = InvalidateRect(hwnd, None, false);
                }
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                // 点击期间记录按下位置，弹起时命中判定
                let _ = SetCapture2(hwnd);
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                let _ = windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
                let x = (lparam.0 & 0xFFFF) as u16 as i32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as u16 as i32;
                let hit = btn_from_point(x, y);
                if hit >= 0 {
                    // 经拥有者（覆盖层）分发动作
                    if let Ok(owner) = windows::Win32::UI::WindowsAndMessaging::GetWindow(
                        hwnd,
                        windows::Win32::UI::WindowsAndMessaging::GW_OWNER,
                    ) {
                        let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                            owner,
                            crate::app::TOOLBAR_ACTION,
                            WPARAM(action_for(hit) as usize),
                            LPARAM(0),
                        );
                    }
                }
                LRESULT(0)
            }
            windows::Win32::UI::WindowsAndMessaging::WM_NCCALCSIZE => {
                // 非 POPUP 形态会被系统附加 WS_CAPTION（白条）：客户区=全窗口
                if wparam.0 != 0 {
                    LRESULT(0)
                } else {
                    DefWindowProcW(hwnd, msg, wparam, lparam)
                }
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_SETCURSOR => {
                let cursor = LoadCursorW(None, IDC_HAND).unwrap_or_default();
                SetCursor(cursor);
                LRESULT(1)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

#[allow(non_snake_case)]
fn SetCapture2(hwnd: HWND) -> HWND {
    unsafe { windows::Win32::UI::Input::KeyboardAndMouse::SetCapture(hwnd) }
}

#[allow(unused)]
fn _unused(p: POINT) -> POINT {
    p
}
