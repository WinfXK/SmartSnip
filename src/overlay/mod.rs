//! Copyright (c) 2026 Winfxk (冰月)
//! Email: admin@winfxk.cn  QQ: 2508543202
//! 创建时间: 2026年11月23日
//!
//! 框选覆盖层：全屏压暗遮罩 + 十字光标 + 选区 + 放大镜。
//! 单 UI 线程驱动，状态机：待拖拽 → 拖拽中 → 完成/取消。

pub mod render;

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateSolidBrush, EndPaint, FillRect, InvalidateRect, StretchDIBits,
    BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, HBRUSH, PAINTSTRUCT, SRCCOPY,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{SetCapture, ReleaseCapture};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetCursorPos, LoadCursorW, RegisterClassW,
    SetCursor, ShowWindow, CW_USEDEFAULT, HWND_TOPMOST, IDC_CROSS, SW_SHOWNOACTIVATE,
    SET_WINDOW_POS_FLAGS, SetWindowPos, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, GetSystemMetrics, WINDOW_EX_STYLE, WINDOW_STYLE,
    WM_CHAR, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_PAINT, WM_RBUTTONUP,
    WM_SETCURSOR, WM_DESTROY, WNDCLASSW,
};

use std::sync::Mutex;

use crate::capture::{capture_region, virtual_desktop_bounds, Frame, Rect};
use crate::app::{SELECTION_DONE};

const CLASS_NAME: &str = "SmartSnip_OverlayWindow";

/// 覆盖层全局状态（单 UI 线程访问）
struct OverlayState {
    hwnd: HWND,
    /// 遮罩前抓取的屏幕原始帧
    source: Frame,
    /// 每次重绘的工作帧（source + 压暗 + 边框 + 放大镜）
    work: Frame,
    /// 桌面坐标系偏移（虚拟桌面左上角）
    origin: (i32, i32),
    /// 拖拽起点（窗口内物理坐标）
    drag_start: Option<(i32, i32)>,
    /// 当前选区（窗口内物理坐标）
    selection: Option<Rect>,
}

static mut STATE: Option<OverlayState> = None;
/// 已确认待消费的选区（桌面物理坐标）；与遮罩状态解耦，避免销毁竞态
static PENDING_SELECTION: Mutex<Option<Rect>> = Mutex::new(None);

/// 启动一次框选会话；完成后向主窗口投递 SELECTION_DONE（选区存于 `take_selection`）
pub fn start() -> Result<(), windows::core::Error> {
    unsafe {
        let bounds = virtual_desktop_bounds();
        let source = capture_region(bounds)?;
        let work = source.clone_shallow();

        let hinstance = GetModuleHandleW(None)?;
        let class_name = windows::core::HSTRING::from(CLASS_NAME);
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance.into(),
            hCursor: LoadCursorW(None, IDC_CROSS).unwrap_or_default(),
            lpszClassName: windows::core::PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        RegisterClassW(&wc);

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0x0000_0020 | 0x0000_0080), // WS_EX_TOPMOST | WS_EX_TOOLWINDOW
            windows::core::PCWSTR(class_name.as_ptr()),
            windows::core::w!(""),
            WINDOW_STYLE(0x8000_0000), // WS_POPUP
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            bounds.width,
            bounds.height,
            None,
            None,
            windows::Win32::Foundation::HINSTANCE(hinstance.0),
            None,
        )?;
        SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            bounds.width,
            bounds.height,
            SET_WINDOW_POS_FLAGS(0x0040), // SWP_SHOWWINDOW
        );
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);

        STATE = Some(OverlayState {
            hwnd,
            source,
            work,
            origin: (
                GetSystemMetrics(SM_XVIRTUALSCREEN),
                GetSystemMetrics(SM_YVIRTUALSCREEN),
            ),
            drag_start: None,
            selection: None,
        });
        let _ = CW_USEDEFAULT;
    }
    Ok(())
}

/// 取出已确认的选区（桌面物理坐标）；一次性消费
pub fn take_selection() -> Option<Rect> {
    PENDING_SELECTION
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .take()
}

fn finish(hwnd: HWND) {
    unsafe {
        let _ = ReleaseCapture();
        let _ = DestroyWindow(hwnd);
        STATE = None;
    }
}

fn redraw() {
    unsafe {
        if let Some(st) = STATE.as_ref() {
            let mut work = st.source.clone_shallow();
            if let Some(sel) = st.selection {
                render::draw_dim(&mut work, sel);
                render::draw_border(&mut work, sel);
            } else {
                // 未选区时整屏压暗
                render::draw_dim(&mut work, Rect { x: -1, y: -1, width: 0, height: 0 });
            }
            // 放大镜：贴窗口左上角固定位置，避免遮挡选区操作
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            let cx = pt.x - st.origin.0;
            let cy = pt.y - st.origin.1;
            render::draw_magnifier(&st.source, &mut work, (cx, cy), (8, 8));
            if let Some(st_mut) = STATE.as_mut() {
                st_mut.work = work;
            }
            let _ = InvalidateRect(st.hwnd, None, false);
        }
    }
}

extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_PAINT => {
                if let Some(st) = STATE.as_ref() {
                    let mut ps = PAINTSTRUCT::default();
                    let hdc = BeginPaint(hwnd, &mut ps);
                    let mut bi = BITMAPINFO::default();
                    bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
                    bi.bmiHeader.biWidth = st.work.width as i32;
                    bi.bmiHeader.biHeight = -(st.work.height as i32);
                    bi.bmiHeader.biPlanes = 1;
                    bi.bmiHeader.biBitCount = 32;
                    bi.bmiHeader.biCompression = DIB_RGB_COLORS.0;
                    StretchDIBits(
                        hdc,
                        0,
                        0,
                        st.work.width as i32,
                        st.work.height as i32,
                        0,
                        0,
                        st.work.width as i32,
                        st.work.height as i32,
                        Some(st.work.data.as_ptr().cast()),
                        &bi,
                        DIB_RGB_COLORS,
                        SRCCOPY,
                    );
                    let _ = EndPaint(hwnd, &ps);
                }
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                let (x, y) = unpack_xy(lparam);
                if let Some(st) = STATE.as_mut() {
                    st.drag_start = Some((x, y));
                    let _ = SetCapture(hwnd);
                }
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                if let Some(st) = STATE.as_mut() {
                    let (x, y) = unpack_xy(lparam);
                    if let Some((sx, sy)) = st.drag_start {
                        st.selection = Some(Rect {
                            x: sx.min(x),
                            y: sy.min(y),
                            width: (sx - x).abs().max(1),
                            height: (sy - y).abs().max(1),
                        });
                    }
                }
                redraw();
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                let confirmed = STATE
                    .as_ref()
                    .is_some_and(|st| st.drag_start.is_some() && st.selection.is_some());
                if confirmed {
                    // 先把选区换算到桌面坐标并暂存，WM_DESTROY 清 STATE 不影响后续取用
                    let pending = STATE.as_ref().map(|st| {
                        let sel = st.selection.unwrap();
                        Rect {
                            x: sel.x + st.origin.0,
                            y: sel.y + st.origin.1,
                            width: sel.width,
                            height: sel.height,
                        }
                    });
                    *PENDING_SELECTION.lock().unwrap_or_else(|p| p.into_inner()) = pending;
                    let _ = ReleaseCapture();
                    let main = crate::app::main_hwnd();
                    let _ = DestroyWindow(hwnd);
                    let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                        main,
                        SELECTION_DONE,
                        WPARAM(0),
                        LPARAM(0),
                    );
                }
                LRESULT(0)
            }
            WM_RBUTTONUP | WM_KEYDOWN => {
                // 右键 / ESC 取消（WM_KEYDOWN 中仅 ESC）
                if msg == WM_KEYDOWN && wparam.0 as u32 != 0x1B {
                    return LRESULT(0);
                }
                finish(hwnd);
                LRESULT(0)
            }
            WM_CHAR => {
                let ch = wparam.0 as u32;
                // Enter 确认全屏选区
                if ch == 0x0D {
                    let pending = STATE.as_ref().map(|st| Rect {
                        x: st.origin.0,
                        y: st.origin.1,
                        width: st.source.width as i32,
                        height: st.source.height as i32,
                    });
                    *PENDING_SELECTION.lock().unwrap_or_else(|p| p.into_inner()) = pending;
                    let main = crate::app::main_hwnd();
                    let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                        main,
                        SELECTION_DONE,
                        WPARAM(0),
                        LPARAM(0),
                    );
                } else if ch == b'c' as u32 || ch == b'C' as u32 {
                    // 颜色拾取：复制光标处 HEX（不截屏退出）
                    let mut pt = POINT::default();
                    let _ = GetCursorPos(&mut pt);
                    if let Some(st) = STATE.as_ref() {
                        let (cx, cy) = (pt.x - st.origin.0, pt.y - st.origin.1);
                        if let Some((r, g, b)) = render::pixel_color(&st.source, cx, cy) {
                            let hex = format!("#{:02X}{:02X}{:02X}", r, g, b);
                            let _ = crate::clipboard::copy_text(&hex);
                            crate::dbg_println!("已复制颜色: {hex}");
                        }
                    }
                }
                LRESULT(0)
            }
            WM_SETCURSOR => {
                let cursor = LoadCursorW(None, IDC_CROSS).unwrap_or_default();
                SetCursor(cursor);
                LRESULT(1)
            }
            WM_DESTROY => {
                STATE = None;
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

fn unpack_xy(lparam: LPARAM) -> (i32, i32) {
    let x = (lparam.0 & 0xFFFF) as u16 as i32;
    let y = ((lparam.0 >> 16) & 0xFFFF) as u16 as i32;
    (x, y)
}

// 保留引用：遮罩填充使用系统画刷路径在 StretchDIBits 全覆盖后不再需要
#[allow(unused)]
fn _unused_brush(c: COLORREF) -> HBRUSH {
    unsafe { CreateSolidBrush(c) }
}

#[allow(unused)]
fn _unused_rect(r: RECT) -> RECT {
    r
}

impl Frame {
    /// 浅拷贝：BGRA 数据整块复制（一次 memcpy，覆盖层每帧可承受）
    pub fn clone_shallow(&self) -> Frame {
        Frame {
            width: self.width,
            height: self.height,
            data: self.data.clone(),
        }
    }
}

#[cfg(test)]
mod live_tests {
    /// 真机覆盖层创建验证
    #[test]
    fn live_overlay_start() {
        match super::start() {
            Ok(()) => println!("overlay ok"),
            Err(e) => println!("overlay failed: {e}"),
        }
    }
}
