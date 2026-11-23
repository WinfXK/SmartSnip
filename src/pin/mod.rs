//! Copyright (c) 2026 Winfxk (冰月)
//! Email: admin@winfxk.cn  QQ: 2508543202
//! 创建时间: 2026年11月23日
//!
//! 贴图便签窗口：无边框置顶、1:1 物理像素、拖动移动、
//! 滚轮缩放（0.2x~5x）、Ctrl+滚轮透明度（40%~100%）、多实例、即用即毁。

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, InvalidateRect, StretchDIBits, PatBlt, BITMAPINFO, BITMAPINFOHEADER,
    DIB_RGB_COLORS, PAINTSTRUCT, SRCCOPY,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow, GetCursorPos,
    GetSystemMetrics, LoadCursorW, PostMessageW, RegisterClassW, SetCursor, SetLayeredWindowAttributes, AppendMenuW,
    TrackPopupMenu, SM_CXSCREEN, SM_CYSCREEN, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE,
    WM_COMMAND, WM_DESTROY, WM_KEYDOWN, WM_LBUTTONDOWN, WM_MOUSEWHEEL, WM_PAINT,
    WM_RBUTTONUP, WM_SETCURSOR, WNDCLASSW,
};

use windows::Win32::Graphics::Gdi::PATCOPY;
use crate::capture::Frame;
use crate::app::{PIN_CLOSED};

const PATCOPY_PIN: windows::Win32::Graphics::Gdi::ROP_CODE = windows::Win32::Graphics::Gdi::ROP_CODE(PATCOPY.0);

const CLASS_NAME: &str = "SmartSnip_PinWindow";
/// 贴图数量上限（防失控）
pub const MAX_PINS: u32 = 20;
/// 缩放范围
const MIN_SCALE_X100: i32 = 20;
const MAX_SCALE_X100: i32 = 500;

static PIN_COUNT: AtomicU32 = AtomicU32::new(0);
/// 存活贴图句柄表（遮罩启动时对其压序）
static PIN_REGISTRY: Mutex<Vec<isize>> = Mutex::new(Vec::new());
static mut PIN_SEQ: u32 = 0;

/// 单张贴图状态（UI 线程访问）
struct PinState {
    frame: Frame,
    /// 缩放百分比
    scale_x100: i32,
    /// 不透明度 0~255
    alpha: u8,
    /// 拖动中：光标相对窗口左上角的偏移
    drag_offset: Option<(i32, i32)>,
}

/// 每个窗口对应一份状态，通过 GWLP_USERDATA 关联
const GWLP_USERDATA: windows::Win32::UI::WindowsAndMessaging::WINDOW_LONG_PTR_INDEX =
    windows::Win32::UI::WindowsAndMessaging::WINDOW_LONG_PTR_INDEX(-21);

fn can_create() -> bool {
    PIN_COUNT.load(Ordering::Relaxed) < MAX_PINS
}

/// 创建贴图窗口；`screen_pos` 为桌面物理坐标
pub fn create(frame: Frame, screen_pos: (i32, i32)) -> Result<HWND, windows::core::Error> {
    if !can_create() {
        crate::dbg_println!("贴图数量已达上限（{MAX_PINS}）");
        return Err(windows::core::Error::from_win32());
    }
    unsafe {
        let hinstance = GetModuleHandleW(None)?;
        let class_name = windows::core::HSTRING::from(CLASS_NAME);
        static REGISTERED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        if REGISTERED.get().is_none() {
            let wc = WNDCLASSW {
                lpfnWndProc: Some(wnd_proc),
                hInstance: hinstance.into(),
                hCursor: LoadCursorW(None, windows::Win32::UI::WindowsAndMessaging::IDC_HAND).unwrap_or_default(),
                lpszClassName: windows::core::PCWSTR(class_name.as_ptr()),
                ..Default::default()
            };
            RegisterClassW(&wc);
            REGISTERED.set(()).ok();
        }

        PIN_SEQ += 1;
        let state = Box::into_raw(Box::new(PinState {
            frame,
            scale_x100: 100,
            alpha: 255,
            drag_offset: None,
        }));

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0x0000_0008 | 0x0000_0080), // WS_EX_TOPMOST | WS_EX_TOOLWINDOW
            windows::core::PCWSTR(class_name.as_ptr()),
            windows::core::w!("SmartSnip Pin"),
            WINDOW_STYLE(0x1000_0000), // WS_VISIBLE（本构建对可见 WS_POPUP 强加 WS_MAXIMIZE，禁用拖动；改用普通窗口形态）
            screen_pos.0,
            screen_pos.1,
            (*state).frame.width as i32,
            (*state).frame.height as i32,
            None,
            None,
            windows::Win32::Foundation::HINSTANCE(hinstance.0),
            Some(state.cast()),
        )?;
        {
            let mut si = windows::Win32::System::Threading::STARTUPINFOW::default();
            windows::Win32::System::Threading::GetStartupInfoW(&mut si);
            crate::logging::info(&format!(
                "startupinfo showWindow={} flags={}",
                si.wShowWindow, si.dwFlags.0
            ));
        }
        PIN_COUNT.fetch_add(1, Ordering::Relaxed);
        // 登记存活贴图，供遮罩启动时压序
        PIN_REGISTRY.lock().unwrap_or_else(|p| p.into_inner()).push(hwnd.0 as isize);
        crate::logging::info(&format!(
            "bisect-registry style=0x{:X}",
            windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
                hwnd,
                windows::Win32::UI::WindowsAndMessaging::WINDOW_LONG_PTR_INDEX(-16)
            )
        ));
        let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowPos(
            hwnd,
            windows::Win32::UI::WindowsAndMessaging::HWND_TOPMOST,
            0, 0, 0, 0,
            windows::Win32::UI::WindowsAndMessaging::SET_WINDOW_POS_FLAGS(0x0001 | 0x0002), // NOSIZE | NOMOVE
        );

        crate::logging::info(&format!(
            "bisect-show style=0x{:X}",
            windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
                hwnd,
                windows::Win32::UI::WindowsAndMessaging::WINDOW_LONG_PTR_INDEX(-16)
            )
        ));
        // 显示调用后再清一次 WS_MAXIMIZE（显示动作可能重新套用启动信息状态），
        // 并按预期几何复位，防止最大化几何污染命中测试
        // 复位窗口 placement（最大化状态会让系统忽略 HTCAPTION 拖动）
        let mut wp = windows::Win32::UI::WindowsAndMessaging::WINDOWPLACEMENT::default();
        wp.length = std::mem::size_of::<windows::Win32::UI::WindowsAndMessaging::WINDOWPLACEMENT>()
            as u32;
        let _ = windows::Win32::UI::WindowsAndMessaging::GetWindowPlacement(hwnd, &mut wp);
        wp.showCmd = 1;
        let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowPlacement(hwnd, &wp);
        let cur_style = windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
            hwnd,
            windows::Win32::UI::WindowsAndMessaging::WINDOW_LONG_PTR_INDEX(-16),
        );
        windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(
            hwnd,
            windows::Win32::UI::WindowsAndMessaging::WINDOW_LONG_PTR_INDEX(-16),
            cur_style & !0x0100_0000,
        );
        let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowPos(
            hwnd,
            windows::Win32::UI::WindowsAndMessaging::HWND_TOP,
            screen_pos.0,
            screen_pos.1,
            (*state).frame.width as i32,
            (*state).frame.height as i32,
            windows::Win32::UI::WindowsAndMessaging::SET_WINDOW_POS_FLAGS(
                0x0004 | 0x0010 | 0x0020, // NOZORDER | NOACTIVATE | FRAMECHANGED
            ),
        );
        crate::logging::info(&format!(
            "bisect-placement style=0x{:X}",
            windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
                hwnd,
                windows::Win32::UI::WindowsAndMessaging::WINDOW_LONG_PTR_INDEX(-16)
            )
        ));
        Ok(hwnd)
    }
}

/// 当前存活贴图数
pub fn count() -> u32 {
    PIN_COUNT.load(Ordering::Relaxed)
}

/// 将所有贴图窗口排到 `above` 句柄之后（即其下方）
pub fn send_all_below(above: HWND) {
    let reg = PIN_REGISTRY.lock().unwrap_or_else(|p| p.into_inner()).clone();
    for h in reg {
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowPos(
                windows::Win32::Foundation::HWND(h as *mut _),
                above,
                0,
                0,
                0,
                0,
                windows::Win32::UI::WindowsAndMessaging::SET_WINDOW_POS_FLAGS(
                    0x0001 | 0x0002 | 0x0010, // NOSIZE | NOMOVE | NOACTIVATE
                ),
            );
        }
    }
}

fn state_from(hwnd: HWND) -> *mut PinState {
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut PinState
    }
}

fn close(hwnd: HWND) {
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
}

extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            windows::Win32::UI::WindowsAndMessaging::WM_NCCREATE => {
                let create = lparam.0 as *mut windows::Win32::UI::WindowsAndMessaging::CREATESTRUCTW;
                let state = (*create).lpCreateParams as *mut PinState;
                windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize);
                // 剥离启动信息附加的 WS_MAXIMIZE，避免最大化状态毒化命中与拖动
                (*create).style &= !0x0100_0000;
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            windows::Win32::UI::WindowsAndMessaging::WM_NCHITTEST => {
                // 整窗视为标题栏：拖动由系统原生处理，不依赖自家消息
                return LRESULT(2); // HTCAPTION
            }
            windows::Win32::UI::WindowsAndMessaging::WM_NCHITTEST => {
                let r = DefWindowProcW(hwnd, msg, wparam, lparam);
                crate::logging::info(&format!("贴图: nchittest={:#X}", r.0));
                // 命中客户区时按标题栏处理，交给系统原生拖动
                if r.0 as u32 == 1 {
                    return LRESULT(2);
                }
                r
            }
            windows::Win32::UI::WindowsAndMessaging::WM_NCCALCSIZE => {
                // [实验A] 暂时走默认路径
                if std::env::var("PIN_KEEP_BORDER").is_ok() {
                    DefWindowProcW(hwnd, msg, wparam, lparam)
                } else if wparam.0 != 0 {
                    LRESULT(0)
                } else {
                    DefWindowProcW(hwnd, msg, wparam, lparam)
                }
            }
            windows::Win32::UI::WindowsAndMessaging::WM_CAPTURECHANGED => {
                // 捕获被夺走时复位拖动态，防止鼠标事件全部被贴图吞掉
                let st = state_from(hwnd);
                if !st.is_null() {
                    (*st).drag_offset = None;
                }
                LRESULT(0)
            }
            WM_PAINT => {
                let st = state_from(hwnd);
                if !st.is_null() {
                    let mut ps = PAINTSTRUCT::default();
                    let hdc = BeginPaint(hwnd, &mut ps);
                    let disp_w = ((*st).frame.width as i32 * (*st).scale_x100 / 100).max(1);
                    let disp_h = ((*st).frame.height as i32 * (*st).scale_x100 / 100).max(1);
                    let mut bi = BITMAPINFO::default();
                    bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
                    bi.bmiHeader.biWidth = (*st).frame.width as i32;
                    bi.bmiHeader.biHeight = -((*st).frame.height as i32);
                    bi.bmiHeader.biPlanes = 1;
                    bi.bmiHeader.biBitCount = 32;
                    bi.bmiHeader.biCompression = DIB_RGB_COLORS.0;
                    let _ = StretchDIBits(
                        hdc, 0, 0, disp_w, disp_h, 0, 0,
                        (*st).frame.width as i32, (*st).frame.height as i32,
                        Some((*st).frame.data.as_ptr().cast()), &bi, DIB_RGB_COLORS, SRCCOPY,
                    );
                    // 1px 反色边框：取左上角像素取反，保证任何底色都可辨
                    let px = &(*st).frame.data;
                    let (r, g, b) = (px[2], px[1], px[0]);
                    let edge = COLORREF((!r) as u32 | ((!g) as u32) << 8 | ((!b) as u32) << 16);
                    let brush = windows::Win32::Graphics::Gdi::CreateSolidBrush(edge);
                    let old = windows::Win32::Graphics::Gdi::SelectObject(
                        hdc,
                        windows::Win32::Graphics::Gdi::HGDIOBJ(brush.0),
                    );
                    let _ = PatBlt(hdc, 0, 0, disp_w, 1, PATCOPY_PIN);
                    let _ = PatBlt(hdc, 0, disp_h - 1, disp_w, 1, PATCOPY_PIN);
                    let _ = PatBlt(hdc, 0, 0, 1, disp_h, PATCOPY_PIN);
                    let _ = PatBlt(hdc, disp_w - 1, 0, 1, disp_h, PATCOPY_PIN);
                    windows::Win32::Graphics::Gdi::SelectObject(hdc, old);
                    let _ = windows::Win32::Graphics::Gdi::DeleteObject(
                        windows::Win32::Graphics::Gdi::HGDIOBJ(brush.0),
                    );
                    let _ = EndPaint(hwnd, &ps);
                }
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                // 手动拖动：记录光标相对窗口偏移并捕获鼠标
                let st = state_from(hwnd);
                if !st.is_null() {
                    let mut pt = POINT::default();
                    let _ = GetCursorPos(&mut pt);
                    let mut wr = RECT::default();
                    let _ = windows::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd, &mut wr);
                    (*st).drag_offset = Some((pt.x - wr.left, pt.y - wr.top));
                    let _ = SetCapturePin(hwnd);
                }
                LRESULT(0)
            }
            windows::Win32::UI::WindowsAndMessaging::WM_MOUSEMOVE => {
                let st = state_from(hwnd);
                if !st.is_null() {
                    if let Some((ox, oy)) = (*st).drag_offset {
                        let mut pt = POINT::default();
                        let _ = GetCursorPos(&mut pt);
                        let ok = windows::Win32::UI::WindowsAndMessaging::SetWindowPos(
                            hwnd,
                            None,
                            pt.x - ox,
                            pt.y - oy,
                            0,
                            0,
                            windows::Win32::UI::WindowsAndMessaging::SET_WINDOW_POS_FLAGS(
                                0x0001 | 0x0004 | 0x0010, // NOSIZE | NOZORDER | NOACTIVATE
                            ),
                        );
                        crate::logging::info(&format!(
                            "pin move to ({},{}) ok={}",
                            pt.x - ox,
                            pt.y - oy,
                            ok.is_ok()
                        ));
                    }
                }
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                let st = state_from(hwnd);
                if !st.is_null() {
                    (*st).drag_offset = None;
                }
                let _ = windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
                LRESULT(0)
            }
            WM_MOUSEWHEEL => {
                let st = state_from(hwnd);
                if !st.is_null() {
                    let ctrl = wparam.0 & (0x0008 << 16) != 0; // MK_CONTROL
                    let delta = ((wparam.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
                    if ctrl {
                        // 透明度：首次调整时动态启用 layered
                        let ex = windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
                            hwnd,
                            windows::Win32::UI::WindowsAndMessaging::WINDOW_LONG_PTR_INDEX(-20),
                        );
                        if ex & 0x0008_0000 == 0 {
                            windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(
                                hwnd,
                                windows::Win32::UI::WindowsAndMessaging::WINDOW_LONG_PTR_INDEX(-20),
                                ex | 0x0008_0000,
                            );
                            SetLayeredWindowAttributes(
                                hwnd,
                                COLORREF(0),
                                (*st).alpha,
                                windows::Win32::UI::WindowsAndMessaging::LAYERED_WINDOW_ATTRIBUTES_FLAGS(0x0000_0002),
                            );
                        }
                        let step = if delta > 0 { 16 } else { -16 };
                        let new = ((*st).alpha as i32 + step).clamp(102, 255); // 40%~100%
                        (*st).alpha = new as u8;
                        SetLayeredWindowAttributes(
                            hwnd,
                            COLORREF(0),
                            new as u8,
                            windows::Win32::UI::WindowsAndMessaging::LAYERED_WINDOW_ATTRIBUTES_FLAGS(0x0000_0002),
                        );
                    } else {
                        // 缩放
                        let step = if delta > 0 { 10 } else { -10 };
                        (*st).scale_x100 =
                            ((*st).scale_x100 + step).clamp(MIN_SCALE_X100, MAX_SCALE_X100);
                        let disp_w = ((*st).frame.width as i32 * (*st).scale_x100 / 100).max(1);
                        let disp_h = ((*st).frame.height as i32 * (*st).scale_x100 / 100).max(1);
                        let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowPos(
                            hwnd, None, 0, 0, disp_w, disp_h,
                            windows::Win32::UI::WindowsAndMessaging::SET_WINDOW_POS_FLAGS(0x0002 | 0x0004), // NOMOVE | NOZORDER
                        );
                        let _ = InvalidateRect(hwnd, None, false);
                    }
                }
                LRESULT(0)
            }
            windows::Win32::UI::WindowsAndMessaging::WM_NCRBUTTONUP => {
                // HTCAPTION 下右键走 NC 消息：弹自定义菜单
                show_pin_menu(hwnd);
                LRESULT(0)
            }
            WM_RBUTTONUP => {
                show_pin_menu(hwnd);
                LRESULT(0)
            }
            WM_KEYDOWN => {
                if wparam.0 as u32 == 0x1B {
                    close(hwnd);
                }
                LRESULT(0)
            }
            WM_COMMAND => {
                match (wparam.0 & 0xFFFF) as u16 {
                    menu::CLOSE => close(hwnd),
                    menu::COPY => {
                        let st = state_from(hwnd);
                        if !st.is_null() {
                            if let Err(err) = crate::clipboard::copy_frame(&(*st).frame) {
                                crate::dbg_println!("{err}");
                            }
                        }
                    }
                    menu::SAVE => {
                        let st = state_from(hwnd);
                        if !st.is_null() {
                            let path = crate::clipboard::default_save_path();
                            if let Err(err) = crate::clipboard::save_frame_png(&(*st).frame, &path) {
                                crate::dbg_println!("{err}");
                            } else {
                                crate::dbg_println!("已保存: {}", path.display());
                            }
                        }
                    }
                    _ => {}
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                let st = state_from(hwnd);
                if !st.is_null() {
                    drop(Box::from_raw(st));
                    windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                }
                PIN_COUNT.fetch_sub(1, Ordering::Relaxed);
                let mut reg = PIN_REGISTRY.lock().unwrap_or_else(|p| p.into_inner());
                reg.retain(|&h| h != hwnd.0 as isize);
                let _ = PostMessageW(crate::app::main_hwnd(), PIN_CLOSED, WPARAM(0), LPARAM(0));
                LRESULT(0)
            }
            WM_SETCURSOR => {
                let cursor = LoadCursorW(None, windows::Win32::UI::WindowsAndMessaging::IDC_HAND).unwrap_or_default();
                SetCursor(cursor);
                LRESULT(1)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

mod menu {
    pub const CLOSE: u16 = 1;
    pub const COPY: u16 = 2;
    pub const SAVE: u16 = 3;
}

fn show_pin_menu(hwnd: HWND) {
    unsafe {
        let menu: windows::Win32::UI::WindowsAndMessaging::HMENU =
            CreatePopupMenu().unwrap_or_default();
        if menu.is_invalid() {
            return;
        }
        let _ = AppendMenuW(menu, windows::Win32::UI::WindowsAndMessaging::MENU_ITEM_FLAGS(0), menu::COPY as usize, windows::core::w!("复制到剪贴板"));
        let _ = AppendMenuW(menu, windows::Win32::UI::WindowsAndMessaging::MENU_ITEM_FLAGS(0), menu::SAVE as usize, windows::core::w!("另存为 PNG"));
        let _ = AppendMenuW(menu, windows::Win32::UI::WindowsAndMessaging::MENU_ITEM_FLAGS(0), menu::CLOSE as usize, windows::core::w!("关闭贴图"));
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let _ = TrackPopupMenu(menu, windows::Win32::UI::WindowsAndMessaging::TPM_RIGHTBUTTON, pt.x, pt.y, 0, hwnd, None);
        let _ = PostMessageW(hwnd, windows::Win32::UI::WindowsAndMessaging::WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
    }
}

/// SetCapture 收口
#[allow(non_snake_case)]
fn SetCapturePin(hwnd: HWND) -> HWND {
    unsafe { windows::Win32::UI::Input::KeyboardAndMouse::SetCapture(hwnd) }
}

// 屏幕尺寸保留用于默认落位越界修正（M4 打磨）
const _: windows::Win32::UI::WindowsAndMessaging::SYSTEM_METRICS_INDEX = SM_CXSCREEN;
const _: windows::Win32::UI::WindowsAndMessaging::SYSTEM_METRICS_INDEX = SM_CYSCREEN;

#[allow(unused)]
fn _unused_rect(r: RECT) -> RECT {
    r
}
