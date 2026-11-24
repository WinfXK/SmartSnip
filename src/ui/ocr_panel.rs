//! Copyright (c) 2026 Winfxk (冰月)
//! Email: admin@winfxk.cn  QQ: 2508543202
//! 创建时间: 2026年11月23日
//!
//! OCR 结果面板：逐行展示识别文本，单击行复制该行，「全部复制」复制全文。
//! 置顶无边框原生绘制，随选区附近弹出。

use std::sync::Mutex;

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect,
    InvalidateRect, SelectObject, SetBkMode, SetTextColor, CLIP_DEFAULT_PRECIS,
    DEFAULT_QUALITY, DT_CALCRECT, DT_CENTER, DT_SINGLELINE, DT_VCENTER, DT_WORDBREAK,
    FF_DONTCARE, FW_NORMAL, HBRUSH, OUT_DEFAULT_PRECIS, PROOF_QUALITY, TRANSPARENT,
    CLEARTYPE_QUALITY,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetSystemMetrics, LoadCursorW,
    RegisterClassW, SetCursor, ShowWindow,
    HWND_TOPMOST, IDC_ARROW, SM_CXSCREEN, SM_CYSCREEN, SW_SHOWNOACTIVATE,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_CREATE, WM_ERASEBKGND, WM_LBUTTONDOWN, WM_PAINT,
    WM_SETCURSOR, WNDCLASSW,
};

const CLASS_NAME: &str = "SmartSnip_OcrPanelWindow";
const PANEL_W: i32 = 340;
const PANEL_MAX_H: i32 = 480;
const HEADER_H: i32 = 36;
const ROW_PAD: i32 = 8;
const LINE_H: i32 = 22;

const BG: COLORREF = COLORREF(0x002A2C29);
const HEADER_BG: COLORREF = COLORREF(0x003D403C);
const ROW_BG: COLORREF = COLORREF(0x00333431);
const ACCENT: COLORREF = COLORREF(0x001E88E5);
const TEXT: COLORREF = COLORREF(0x00F0F0F0);

static LINES: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// 每行渲染所需高度（WM_PAINT 计算后缓存给命中测试）
static mut ROW_HEIGHTS: Vec<i32> = Vec::new();

/// 展示识别结果；出现在选区附近
pub fn show(lines: Vec<String>, near: (i32, i32)) -> Result<HWND, windows::core::Error> {
    unsafe {
        *LINES.lock().unwrap_or_else(|p| p.into_inner()) = lines;
        let hinstance = GetModuleHandleW(None)?;
        let class_name = windows::core::HSTRING::from(CLASS_NAME);
        static REGISTERED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        if REGISTERED.get().is_none() {
            let wc = WNDCLASSW {
                lpfnWndProc: Some(wnd_proc),
                hInstance: hinstance.into(),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                lpszClassName: windows::core::PCWSTR(class_name.as_ptr()),
                ..Default::default()
            };
            RegisterClassW(&wc);
            REGISTERED.set(()).ok();
        }

        // 高度按内容估算（保守值，超出滚动后续迭代）
        let n = LINES.lock().unwrap().len() as i32;
        let h = (HEADER_H + n * (LINE_H + ROW_PAD) + ROW_PAD * 2).clamp(HEADER_H + 60, PANEL_MAX_H);
        let (sw, sh) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
        let mut x = near.0 + 20;
        let mut y = near.1 + 20;
        if x + PANEL_W > sw {
            x = sw - PANEL_W - 20;
        }
        if y + h > sh {
            y = sh - h - 20;
        }
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0x0000_0008 | 0x0000_0080), // WS_EX_TOPMOST | WS_EX_TOOLWINDOW
            windows::core::PCWSTR(class_name.as_ptr()),
            windows::core::w!(""),
            WINDOW_STYLE(0x8000_0000 | 0x1000_0000), // WS_POPUP | WS_VISIBLE
            x,
            y,
            PANEL_W,
            h,
            None,
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
        windows::Win32::Graphics::Gdi::DEFAULT_CHARSET.0 as u32,
        OUT_DEFAULT_PRECIS.0 as u32,
        CLIP_DEFAULT_PRECIS.0 as u32,
        CLEARTYPE_QUALITY.0 as u32,
        FF_DONTCARE.0 as u32,
        windows::core::PCWSTR(name.as_ptr()),
    )
}

fn header_buttons() -> [(RECT, &'static str); 2] {
    [
        (RECT { left: 10, top: 6, right: 90, bottom: HEADER_H - 6 }, "全部复制"),
        (RECT { left: PANEL_W - 80, top: 6, right: PANEL_W - 10, bottom: HEADER_H - 6 }, "关闭"),
    ]
}

extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_CREATE => {
                // 去除 DWM 边框感
                LRESULT(0)
            }
            WM_PAINT => {
                let mut ps = windows::Win32::Graphics::Gdi::PAINTSTRUCT::default();
                let hdc = windows::Win32::Graphics::Gdi::BeginPaint(hwnd, &mut ps);
                let bg = CreateSolidBrush(BG);
                FillRect(hdc, &ps.rcPaint, bg);
                let _ = DeleteObject(windows::Win32::Graphics::Gdi::HGDIOBJ(bg.0));

                let font = make_font();
                let old_font = SelectObject(hdc, windows::Win32::Graphics::Gdi::HGDIOBJ(font.0));
                SetBkMode(hdc, TRANSPARENT);

                // 头部按钮
                let header_brush = CreateSolidBrush(HEADER_BG);
                let accent = CreateSolidBrush(ACCENT);
                for (i, (r, label)) in header_buttons().iter().enumerate() {
                    let brush = if i == 0 { accent } else { header_brush };
                    FillRect(hdc, r, brush);
                    SetTextColor(hdc, TEXT);
                    let mut t: Vec<u16> = label.encode_utf16().collect();
                    t.push(0);
                    let mut rr = *r;
                    let _ = DrawTextW(hdc, &mut t, &mut rr, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
                }

                // 行内容
                let lines = LINES.lock().unwrap_or_else(|p| p.into_inner()).clone();
                let mut heights: Vec<i32> = Vec::with_capacity(lines.len());
                let mut y = HEADER_H;
                let row_brush = CreateSolidBrush(ROW_BG);
                for (idx, line) in lines.iter().enumerate() {
                    let mut t: Vec<u16> = line.encode_utf16().collect();
                    let pad = ROW_PAD;
                    let mut rc = RECT {
                        left: 10,
                        top: y + pad,
                        right: PANEL_W - 10,
                        bottom: 0,
                    };
                    SetTextColor(hdc, TEXT);
                    let _ = DrawTextW(hdc, &mut t, &mut rc, DT_CALCRECT | DT_WORDBREAK);
                    let row_h = (rc.bottom - rc.top) + pad * 2;
                    let row_rect = RECT {
                        left: 6,
                        top: y,
                        right: PANEL_W - 6,
                        bottom: y + row_h,
                    };
                    FillRect(hdc, &row_rect, row_brush);
                    let mut t2: Vec<u16> = format!("{}  {}", idx + 1, line).encode_utf16().collect();
                    let mut rc2 = RECT {
                        left: 14,
                        top: y + pad,
                        right: PANEL_W - 14,
                        bottom: y + row_h - pad,
                    };
                    let _ = DrawTextW(hdc, &mut t2, &mut rc2, DT_WORDBREAK);
                    heights.push(row_h);
                    y += row_h;
                }
                let _ = DeleteObject(windows::Win32::Graphics::Gdi::HGDIOBJ(row_brush.0));
                let _ = DeleteObject(windows::Win32::Graphics::Gdi::HGDIOBJ(accent.0));
                let _ = DeleteObject(windows::Win32::Graphics::Gdi::HGDIOBJ(header_brush.0));

                ROW_HEIGHTS = heights;
                SelectObject(hdc, old_font);
                let _ = DeleteObject(windows::Win32::Graphics::Gdi::HGDIOBJ(font.0));
                let _ = windows::Win32::Graphics::Gdi::EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                let x = (lparam.0 & 0xFFFF) as u16 as i32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as u16 as i32;
                // 头部按钮
                for (i, (r, _)) in header_buttons().iter().enumerate() {
                    if x >= r.left && x < r.right && y >= r.top && y < r.bottom {
                        if i == 1 {
                            destroy(hwnd);
                        } else {
                            let lines = LINES.lock().unwrap_or_else(|p| p.into_inner()).join("\n");
                            let _ = crate::clipboard::copy_text(&lines);
                        }
                        return LRESULT(0);
                    }
                }
                // 数据行 → 复制该行
                if y > HEADER_H {
                    let lines = LINES.lock().unwrap_or_else(|p| p.into_inner()).clone();
                    let mut acc = HEADER_H;
                    for (idx, h) in ROW_HEIGHTS.iter().enumerate() {
                        if y >= acc && y < acc + h {
                            if let Some(line) = lines.get(idx) {
                                let _ = crate::clipboard::copy_text(line);
                            }
                            break;
                        }
                        acc += h;
                    }
                }
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_SETCURSOR => {
                let cursor = LoadCursorW(None, IDC_ARROW).unwrap_or_default();
                SetCursor(cursor);
                LRESULT(1)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
