//! Copyright (c) 2026 Winfxk (冰月)
//! Email: admin@winfxk.cn  QQ: 2508543202
//! 创建时间: 2026年11月23日
//!
//! 框选覆盖层：全屏压暗遮罩 + 十字光标 + 选区 + 放大镜。
//! 渲染策略：原图与压暗底图各持一份 DIB 位图，WM_PAINT 按脏区做
//! 「底图 blit → 选区内原图 blit → 边框/放大镜」，拖动零全屏光栅化。

pub mod render;
pub mod toolbar;

use std::sync::Mutex;

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    AlphaBlend, BeginPaint, BitBlt, CreateCompatibleDC, CreateDCW, CreateDIBSection, CreatePen,
    CreateSolidBrush, DeleteDC, DeleteObject, EndPaint, GetPixel, GetStockObject, InvalidateRect,
    PatBlt, Polyline, Rectangle, SelectObject, StretchBlt, AC_SRC_OVER, BITMAPINFO,
    BITMAPINFOHEADER, BLENDFUNCTION, DIB_RGB_COLORS, HBRUSH, HBITMAP, HDC, HOLLOW_BRUSH,
    PAINTSTRUCT, PS_SOLID, SRCCOPY,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetCursorPos, LoadCursorW, RegisterClassW,
    SetCursor, ShowWindow, HWND_TOPMOST, IDC_CROSS, SW_SHOWNOACTIVATE, SET_WINDOW_POS_FLAGS,
    SetWindowPos, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    GetSystemMetrics, SetForegroundWindow, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CHAR,
    WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_PAINT, WM_RBUTTONUP,
    WM_SETCURSOR, WM_DESTROY, WNDCLASSW,
};

use crate::app::SELECTION_DONE;
use crate::capture::{capture_region, virtual_desktop_bounds, Rect};

const CLASS_NAME: &str = "SmartSnip_OverlayWindow";

/// 覆盖层全局状态（单 UI 线程访问）
struct OverlayState {
    hwnd: HWND,
    /// 桌面坐标系偏移（虚拟桌面左上角）
    origin: (i32, i32),
    /// 屏幕物理尺寸
    screen: (i32, i32),
    /// 原图内存 DC（含 DIB 位图）
    src_dc: HDC,
    /// 待释放的 GDI 位图
    src_bmp: HBITMAP,
    /// 1x1 纯黑源（压暗用，随脏区拉伸 AlphaBlend）
    dim_src_dc: HDC,
    dim_src_bmp: HBITMAP,
    /// 最近一帧光标位置（窗口内物理坐标）
    cursor: (i32, i32),
    /// 拖拽起点（窗口内物理坐标）
    drag_start: Option<(i32, i32)>,
    /// 当前选区（窗口内物理坐标）
    selection: Option<Rect>,
    /// 上次绘制的边框外扩矩形（脏区计算）
    prev_border: Option<Rect>,
    /// 首帧已全屏绘制
    first_paint_done: bool,
    /// 离屏合成缓冲（与屏幕同尺寸，消除多次 blit 的中间状态闪烁）
    paint_dc: HDC,
    paint_bmp: HBITMAP,
    /// 选区已确认，等待工具条动作
    pending: bool,
    /// 工具条窗口
    toolbar: HWND,
    /// 标注层（选区局部坐标）
    annotations: crate::annotate::AnnotationLayer,
    /// 处于标注模式
    annotating: bool,
    /// 当前标注工具
    annot_tool: AnnotTool,
    /// 当前标注色
    annot_color: crate::annotate::AnnotateColor,
    /// 进行中的标注（实时预览）
    annot_preview: Option<crate::annotate::Annotation>,
    /// 上次预览脏区（覆盖层坐标）
    annot_dirty: Option<Rect>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AnnotTool {
    Pen,
    Rect,
}

static mut STATE: Option<OverlayState> = None;
/// 已确认待消费的选区（桌面物理坐标）；与遮罩状态解耦，避免销毁竞态
static PENDING_SELECTION: Mutex<Option<Rect>> = Mutex::new(None);

/// 由 BGRA 帧创建内存 DC + DIB 位图
unsafe fn frame_to_dc(frame: &crate::capture::Frame, hdc_screen: HDC) -> (HDC, HBITMAP) {
    let mut bi = BITMAPINFO::default();
    bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bi.bmiHeader.biWidth = frame.width as i32;
    bi.bmiHeader.biHeight = -(frame.height as i32);
    bi.bmiHeader.biPlanes = 1;
    bi.bmiHeader.biBitCount = 32;
    bi.bmiHeader.biCompression = DIB_RGB_COLORS.0;

    let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
    let bmp = CreateDIBSection(hdc_screen, &bi, DIB_RGB_COLORS, &mut bits, None, 0)
        .unwrap_or_default();
    let mem_dc = CreateCompatibleDC(hdc_screen);
    SelectObject(mem_dc, windows::Win32::Graphics::Gdi::HGDIOBJ(bmp.0));
    if !bits.is_null() {
        core::ptr::copy_nonoverlapping(frame.data.as_ptr(), bits as *mut u8, frame.data.len());
    }
    (mem_dc, bmp)
}

/// 启动一次框选会话；完成后向主窗口投递 SELECTION_DONE
pub fn start() -> Result<(), windows::core::Error> {
    unsafe {
        // 已有遮罩会话时忽略重复触发，避免覆盖 STATE 泄漏窗口
        if STATE.is_some() {
            return Ok(());
        }
        // 同一线程：清除贴图等窗口可能遗留的鼠标捕获，避免遮罩收不到鼠标
        let _ = ReleaseCapture();
        let bounds = virtual_desktop_bounds();
        let source = capture_region(bounds)?;

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
            WINDOW_EX_STYLE(0x0000_0008 | 0x0000_0080), // WS_EX_TOPMOST | WS_EX_TOOLWINDOW
            windows::core::PCWSTR(class_name.as_ptr()),
            windows::core::w!(""),
            WINDOW_STYLE(0x8000_0000), // WS_POPUP
            bounds.x,
            bounds.y,
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
            bounds.x,
            bounds.y,
            bounds.width,
            bounds.height,
            SET_WINDOW_POS_FLAGS(0x0040), // SWP_SHOWWINDOW
        );
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        // 截图会话需要键盘（ESC/Enter/取色），显式夺取前台与焦点
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(hwnd);
        // 新增的顶层置顶窗口已位于所有贴图之上；显式压序作为双保险
        crate::pin::send_all_below(hwnd);
        crate::logging::info(&format!(
            "遮罩: 窗口已显示，贴图压序完成（{} 张）",
            crate::pin::count()
        ));

        let screen_dc = CreateDCW(windows::core::w!("DISPLAY"), None, None, None);
        let (src_dc, src_bmp) = frame_to_dc(&source, screen_dc);
        // 1x1 纯黑位图：压暗时按脏区拉伸 AlphaBlend，避免再存一整屏压暗图
        let (dim_src_dc, dim_src_bmp) = {
            let mut bi = BITMAPINFO::default();
            bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            bi.bmiHeader.biWidth = 1;
            bi.bmiHeader.biHeight = -1;
            bi.bmiHeader.biPlanes = 1;
            bi.bmiHeader.biBitCount = 32;
            bi.bmiHeader.biCompression = DIB_RGB_COLORS.0;
            let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
            let bmp = windows::Win32::Graphics::Gdi::CreateDIBSection(
                screen_dc, &bi, DIB_RGB_COLORS, &mut bits, None, 0,
            )
            .unwrap_or_default();
            if !bits.is_null() {
                // BGRA = 黑
                core::ptr::write_bytes(bits as *mut u8, 0, 4);
            }
            let dc = CreateCompatibleDC(screen_dc);
            SelectObject(dc, windows::Win32::Graphics::Gdi::HGDIOBJ(bmp.0));
            (dc, bmp)
        };
        // 离屏合成缓冲
        let mut bi = BITMAPINFO::default();
        bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bi.bmiHeader.biWidth = bounds.width;
        bi.bmiHeader.biHeight = -bounds.height;
        bi.bmiHeader.biPlanes = 1;
        bi.bmiHeader.biBitCount = 32;
        bi.bmiHeader.biCompression = DIB_RGB_COLORS.0;
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let paint_bmp = windows::Win32::Graphics::Gdi::CreateDIBSection(
            screen_dc, &bi, DIB_RGB_COLORS, &mut bits, None, 0,
        )
        .unwrap_or_default();
        let paint_dc = CreateCompatibleDC(screen_dc);
        SelectObject(paint_dc, windows::Win32::Graphics::Gdi::HGDIOBJ(paint_bmp.0));
        let _ = DeleteDC(screen_dc);

        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);

        crate::logging::info("遮罩: 就绪");
        STATE = Some(OverlayState {
            hwnd,
            origin: (
                GetSystemMetrics(SM_XVIRTUALSCREEN),
                GetSystemMetrics(SM_YVIRTUALSCREEN),
            ),
            screen: (bounds.width, bounds.height),
            src_dc,
            src_bmp,
            dim_src_dc,
            dim_src_bmp,
            cursor: (
                pt.x - GetSystemMetrics(SM_XVIRTUALSCREEN),
                pt.y - GetSystemMetrics(SM_YVIRTUALSCREEN),
            ),
            drag_start: None,
            selection: None,
            prev_border: None,
            first_paint_done: false,
            paint_dc,
            paint_bmp,
            pending: false,
            toolbar: HWND::default(),
            annotations: crate::annotate::AnnotationLayer::default(),
            annotating: false,
            annot_tool: AnnotTool::Pen,
            annot_color: crate::annotate::AnnotateColor::from_name(
                &crate::config::runtime().annotate.color,
            ),
            annot_preview: None,
            annot_dirty: None,
        });
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

fn cleanup_gdi(st: &OverlayState) {
    unsafe {
        let _ = DeleteObject(windows::Win32::Graphics::Gdi::HGDIOBJ(st.src_bmp.0));
        let _ = DeleteObject(windows::Win32::Graphics::Gdi::HGDIOBJ(st.dim_src_bmp.0));
        let _ = DeleteDC(st.src_dc);
        let _ = DeleteDC(st.dim_src_dc);
        let _ = DeleteObject(windows::Win32::Graphics::Gdi::HGDIOBJ(st.paint_bmp.0));
        let _ = DeleteDC(st.paint_dc);
    }
}

fn finish(hwnd: HWND) {
    unsafe {
        let _ = ReleaseCapture();
        if let Some(st) = STATE.as_ref() {
            if !st.toolbar.is_invalid() {
                toolbar::destroy(st.toolbar);
            }
            cleanup_gdi(st);
        }
        let _ = DestroyWindow(hwnd);
        STATE = None;
    }
}

/// 边框外扩矩形（含厚度余量）
fn border_rect(sel: Rect) -> Rect {
    Rect {
        x: sel.x - render::BORDER_WIDTH,
        y: sel.y - render::BORDER_WIDTH,
        width: sel.width + render::BORDER_WIDTH * 2,
        height: sel.height + render::BORDER_WIDTH * 2,
    }
}

/// 两矩形并集
fn union(a: Rect, b: Rect) -> Rect {
    let x0 = a.x.min(b.x);
    let y0 = a.y.min(b.y);
    let x1 = (a.x + a.width).max(b.x + b.width);
    let y1 = (a.y + a.height).max(b.y + b.height);
    Rect { x: x0, y: y0, width: x1 - x0, height: y1 - y0 }
}

/// 触发局部重绘：旧边框区 ∪ 新边框区 ∪ 放大镜区
fn redraw() {
    unsafe {
        let Some(st) = STATE.as_ref() else { return };
        let new_border = st.selection.map(border_rect);
        let mut dirty = match (st.prev_border, new_border) {
            (Some(a), Some(b)) => Some(union(a, b)),
            (Some(a), None) | (None, Some(a)) => Some(a),
            (None, None) => None,
        };
        let mag = mag_rect();
        dirty = Some(match dirty {
            Some(d) => union(d, mag),
            None => mag,
        });
        let _ = InvalidateRect(st.hwnd, Some(&dirty.unwrap().to_win32()), false);
        if let Some(st_mut) = STATE.as_mut() {
            st_mut.prev_border = new_border;
        }
    }
}

fn mag_rect() -> Rect {
    Rect {
        x: render::MAG_ORIGIN.0,
        y: render::MAG_ORIGIN.1,
        width: render::MAG_SIZE,
        height: render::MAG_SIZE,
    }
}

/// 按脏区合成一帧画面
unsafe fn paint(hdc: HDC, rc: &RECT) {
    let Some(st) = STATE.as_ref() else { return };
    let dirty = Rect {
        x: rc.left,
        y: rc.top,
        width: rc.right - rc.left,
        height: rc.bottom - rc.top,
    };
    // 1. 脏区先铺原图
    let _ = BitBlt(
        hdc,
        dirty.x,
        dirty.y,
        dirty.width,
        dirty.height,
        st.src_dc,
        dirty.x,
        dirty.y,
        SRCCOPY,
    );
    // 2. 再叠 50% 黑压暗（1x1 源拉伸，省去整屏压暗位图）
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 128,
        AlphaFormat: 0,
    };
    let _ = AlphaBlend(
        hdc,
        dirty.x,
        dirty.y,
        dirty.width,
        dirty.height,
        st.dim_src_dc,
        0,
        0,
        1,
        1,
        blend,
    );
    // 2. 选区内还原原图
    if let Some(sel) = st.selection {
        let ix0 = sel.x.max(dirty.x);
        let iy0 = sel.y.max(dirty.y);
        let ix1 = (sel.x + sel.width).min(dirty.x + dirty.width);
        let iy1 = (sel.y + sel.height).min(dirty.y + dirty.height);
        if ix1 > ix0 && iy1 > iy0 {
            let _ = BitBlt(hdc, ix0, iy0, ix1 - ix0, iy1 - iy0, st.src_dc, ix0, iy0, SRCCOPY);
        }
        // 3. 边框（实心 2px）
        let brush = border_brush();
        let outer = border_rect(sel);
        pat_blt_frame(hdc, &outer, &sel, brush);
        let _ = DeleteObject(windows::Win32::Graphics::Gdi::HGDIOBJ(brush.0));
        // 3.5 标注（选区局部坐标 → 屏幕偏移）
        for item in st.annotations.items() {
            draw_annotation(hdc, item, sel.x, sel.y);
        }
        if let Some(preview) = st.annot_preview.as_ref() {
            draw_annotation(hdc, preview, sel.x, sel.y);
        }
    }
    // 4. 放大镜（标注模式下隐藏，避免遮挡绘制）
    if st.annotating {
        return;
    }
    let mag = mag_rect();
    let src = render::mag_source_rect(st.cursor, st.screen.0, st.screen.1);
    let brush = border_brush();
    let old = SelectObject(hdc, windows::Win32::Graphics::Gdi::HGDIOBJ(brush.0));
    let _ = StretchBlt(
        hdc,
        mag.x, mag.y, mag.width, mag.height,
        st.src_dc,
        src.x, src.y, src.width.max(1), src.height.max(1),
        SRCCOPY,
    );
    pat_rect(hdc, &Rect { x: mag.x, y: mag.y, width: mag.width, height: 2 }, brush);
    pat_rect(hdc, &Rect { x: mag.x, y: mag.y + mag.height - 2, width: mag.width, height: 2 }, brush);
    pat_rect(hdc, &Rect { x: mag.x, y: mag.y, width: 2, height: mag.height }, brush);
    pat_rect(hdc, &Rect { x: mag.x + mag.width - 2, y: mag.y, width: 2, height: mag.height }, brush);
    SelectObject(hdc, old);
    let _ = DeleteObject(windows::Win32::Graphics::Gdi::HGDIOBJ(brush.0));
}

/// 将一条标注绘制到屏幕 DC（局部坐标 + 选区偏移）
unsafe fn draw_annotation(hdc: HDC, a: &crate::annotate::Annotation, ox: i32, oy: i32) {
    use crate::annotate::{AnnotateColor, Annotation};
    use windows::Win32::Graphics::Gdi::HGDIOBJ;

    let colorref = |c: AnnotateColor| {
        let [r, g, b] = c.rgb();
        COLORREF(r as u32 | (g as u32) << 8 | (b as u32) << 16)
    };
    match a {
        Annotation::Pen { points, color, width } => {
            let Some(&first) = points.first() else { return };
            let pts: Vec<POINT> = points
                .iter()
                .map(|&(x, y)| POINT { x: x + ox, y: y + oy })
                .collect();
            let pen = CreatePen(PS_SOLID, (*width).max(1), colorref(*color));
            let old = SelectObject(hdc, HGDIOBJ(pen.0));
            if pts.len() == 1 {
                let _ = Rectangle(
                    hdc,
                    first.0 + ox,
                    first.1 + oy,
                    first.0 + ox + 1,
                    first.1 + oy + 1,
                );
            } else {
                let _ = Polyline(hdc, &pts);
            }
            SelectObject(hdc, old);
            let _ = DeleteObject(HGDIOBJ(pen.0));
        }
        Annotation::Rect { x0, y0, x1, y1, color, width } => {
            let pen = CreatePen(PS_SOLID, (*width).max(1), colorref(*color));
            let old_pen = SelectObject(hdc, HGDIOBJ(pen.0));
            let old_brush = SelectObject(hdc, GetStockObject(HOLLOW_BRUSH));
            let _ = Rectangle(hdc, *x0 + ox, *y0 + oy, *x1 + ox, *y1 + oy);
            SelectObject(hdc, old_brush);
            SelectObject(hdc, old_pen);
            let _ = DeleteObject(HGDIOBJ(pen.0));
        }
        Annotation::Text { .. } => {}
    }
}

/// 标注在选区局部坐标下的包围盒（用于精确重绘）
fn annotation_bounds(a: &crate::annotate::Annotation) -> Option<Rect> {
    use crate::annotate::Annotation;
    match a {
        Annotation::Pen { points, width, .. } => {
            let first = points.first().copied()?;
            let (mut x0, mut y0, mut x1, mut y1) = (first.0, first.1, first.0, first.1);
            for &(x, y) in points {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
            let pad = (*width).max(1);
            Some(Rect {
                x: x0 - pad,
                y: y0 - pad,
                width: (x1 - x0) + pad * 2,
                height: (y1 - y0) + pad * 2,
            })
        }
        Annotation::Rect { x0, y0, x1, y1, width, .. } => {
            let pad = (*width).max(1);
            let (left, right) = ((*x0).min(*x1), (*x0).max(*x1));
            let (top, bottom) = ((*y0).min(*y1), (*y0).max(*y1));
            Some(Rect {
                x: left - pad,
                y: top - pad,
                width: (right - left) + pad * 2,
                height: (bottom - top) + pad * 2,
            })
        }
        Annotation::Text { .. } => None,
    }
}

/// 屏幕坐标 → 选区局部坐标（越界返回 None）
fn local_point(st: &OverlayState, x: i32, y: i32) -> Option<(i32, i32)> {
    let sel = st.selection?;
    let lx = x - sel.x;
    let ly = y - sel.y;
    if lx >= 0 && ly >= 0 && lx < sel.width && ly < sel.height {
        Some((lx, ly))
    } else {
        None
    }
}

/// 预览标注的屏幕坐标包围盒
fn preview_bounds(st: &OverlayState) -> Option<Rect> {
    let sel = st.selection?;
    let b = annotation_bounds(st.annot_preview.as_ref()?)?;
    Some(Rect {
        x: b.x + sel.x,
        y: b.y + sel.y,
        width: b.width,
        height: b.height,
    })
}

fn border_brush() -> HBRUSH {
    unsafe {
        CreateSolidBrush(COLORREF(
            render::BORDER_COLOR[0] as u32
                | (render::BORDER_COLOR[1] as u32) << 8
                | (render::BORDER_COLOR[2] as u32) << 16,
        ))
    }
}

/// 沿选区四边绘制边框（brush 已选入 hdc）
unsafe fn pat_blt_frame(hdc: HDC, outer: &Rect, sel: &Rect, brush: HBRUSH) {
    let t = render::BORDER_WIDTH;
    pat_rect(hdc, &Rect { x: outer.x, y: outer.y, width: outer.width, height: t }, brush);
    pat_rect(hdc, &Rect { x: outer.x, y: sel.y + sel.height, width: outer.width, height: t }, brush);
    pat_rect(hdc, &Rect { x: outer.x, y: outer.y, width: t, height: outer.height }, brush);
    pat_rect(hdc, &Rect { x: sel.x + sel.width, y: outer.y, width: t, height: outer.height }, brush);
}

unsafe fn pat_rect(hdc: HDC, r: &Rect, brush: HBRUSH) {
    let old = SelectObject(hdc, windows::Win32::Graphics::Gdi::HGDIOBJ(brush.0));
    let _ = PatBlt(
        hdc,
        r.x,
        r.y,
        r.width,
        r.height,
        windows::Win32::Graphics::Gdi::ROP_CODE(windows::Win32::Graphics::Gdi::PATCOPY.0),
    );
    SelectObject(hdc, old);
}

/// 选区的桌面坐标
fn selection_desktop(st: &OverlayState) -> Option<Rect> {
    let sel = st.selection?;
    Some(Rect {
        x: sel.x + st.origin.0,
        y: sel.y + st.origin.1,
        width: sel.width,
        height: sel.height,
    })
}

/// 进入标注模式：切换为标注工具条
unsafe fn enter_annotate_mode(hwnd: HWND) {
    let Some(st) = STATE.as_mut() else { return };
    let Some(sel_desktop) = selection_desktop(st) else { return };
    st.annotating = true;
    st.annot_preview = None;
    st.annot_dirty = None;
    toolbar::destroy(st.toolbar);
    match toolbar::show(hwnd, sel_desktop, toolbar::Kind::Annotate) {
        Ok(tb) => st.toolbar = tb,
        Err(err) => crate::logging::error(&format!("标注工具条创建失败: {err}")),
    }
}

/// 退出标注模式：恢复主工具条并重绘选区
unsafe fn leave_annotate_mode(hwnd: HWND) {
    let Some(st) = STATE.as_mut() else { return };
    st.annotating = false;
    st.annot_preview = None;
    st.annot_dirty = None;
    toolbar::destroy(st.toolbar);
    st.toolbar = HWND::default();
    if let Some(sel_desktop) = selection_desktop(st) {
        if let Ok(tb) = toolbar::show(hwnd, sel_desktop, toolbar::Kind::Main) {
            st.toolbar = tb;
        }
    }
    if let Some(sel) = st.selection {
        let _ = InvalidateRect(hwnd, Some(&sel.to_win32()), false);
    }
}

/// 处理标注模式下的工具条动作；返回 true 表示已消费
unsafe fn handle_annotate_action(hwnd: HWND, action: u32) -> bool {
    let Some(st) = STATE.as_mut() else { return false };
    match action {
        toolbar::action::PEN => st.annot_tool = AnnotTool::Pen,
        toolbar::action::RECT => st.annot_tool = AnnotTool::Rect,
        toolbar::action::COLOR => st.annot_color = st.annot_color.next(),
        toolbar::action::UNDO => {
            st.annotations.undo();
            if let Some(sel) = st.selection {
                let _ = InvalidateRect(hwnd, Some(&sel.to_win32()), false);
            }
        }
        toolbar::action::DONE => leave_annotate_mode(hwnd),
        _ => return false,
    }
    true
}

/// 抓取选区并叠加标注
unsafe fn capture_annotated(
    st: &OverlayState,
    desktop: Rect,
) -> windows::core::Result<crate::capture::Frame> {
    let mut frame = capture_region(desktop)?;
    st.annotations.render(&mut frame);
    Ok(frame)
}

/// 工具条动作：以当前选区执行 截图/贴图/OCR/二维码
unsafe fn handle_toolbar_action(hwnd: HWND, action: u32) {
    let Some(st) = STATE.as_ref() else {
        crate::logging::error("遮罩: 动作处理时状态已丢失");
        return;
    };
    if !st.pending {
        return;
    }
    let Some(sel) = st.selection else { return };
    let desktop = Rect {
        x: sel.x + st.origin.0,
        y: sel.y + st.origin.1,
        width: sel.width,
        height: sel.height,
    };
    match action {
        toolbar::action::SNIP => {
            match capture_annotated(st, desktop) {
                Ok(frame) => {
                    if let Err(err) = crate::clipboard::copy_frame(&frame) {
                        crate::logging::error(&err);
                    } else {
                        crate::logging::info("已复制到剪贴板");
                    }
                }
                Err(err) => crate::logging::error(&format!("截图失败: {err}")),
            }
            finish(hwnd);
        }
        toolbar::action::PIN => {
            match capture_annotated(st, desktop) {
                Ok(frame) => {
                    match crate::pin::create(frame, (desktop.x, desktop.y)) {
                        Ok(_) => crate::logging::info("贴图已创建"),
                        Err(err) => crate::logging::error(&format!("贴图失败: {err}")),
                    }
                }
                Err(err) => crate::logging::error(&format!("截图失败: {err}")),
            }
            finish(hwnd);
        }
        toolbar::action::OCR => {
            match capture_annotated(st, desktop) {
                Ok(frame) => {
                    // 后台识别，10s 硬超时；完成后主窗口弹结果
                    let main = crate::app::main_hwnd();
                    let main_isize = main.0 as isize;
                    std::thread::spawn(move || {
                        let main = windows::Win32::Foundation::HWND(main_isize as *mut _);
                        let result = crate::scheduler::with_timeout(
                            move || crate::ocr::recognize(&frame),
                            crate::scheduler::LONG_TASK_TIMEOUT,
                        )
                        .unwrap_or_else(|| Err("识别超时".into()));
                        *crate::app::OCR_PENDING.lock().unwrap_or_else(|p| p.into_inner()) =
                            Some(result);
                        let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                            main,
                            crate::app::OCR_RESULT,
                            WPARAM(0),
                            LPARAM(0),
                        );
                    });
                }
                Err(err) => crate::logging::error(&format!("截图失败: {err}")),
            }
            finish(hwnd);
        }
        toolbar::action::QR => {
            match capture_annotated(st, desktop) {
                Ok(frame) => match crate::qrcode::decode(&frame) {
                    Some(crate::qrcode::QrContent::Text(text)) => {
                        let _ = crate::clipboard::copy_text(&text);
                        crate::logging::info("二维码文本已入剪贴板");
                        message_box(hwnd, &text, "二维码识别", false);
                    }
                    Some(crate::qrcode::QrContent::Jump(target)) => {
                        let risk = crate::qrcode::is_high_risk(&target);
                        let msg = if risk {
                            format!("\u{26a0} 高危协议链接，请谨慎确认：\n\n{target}")
                        } else {
                            format!("是否打开以下链接？\n\n{target}")
                        };
                        let title = if risk { "二维码识别（存在风险）" } else { "二维码识别" };
                        let choice = message_box(hwnd, &msg, title, true);
                        if choice == IDYES {
                            if let Err(err) = crate::qrcode::open_jump(&target) {
                                crate::logging::error(&format!("跳转失败: {err}"));
                            }
                        }
                    }
                    None => {
                        message_box(hwnd, "未在选区内发现二维码", "二维码识别", false);
                    }
                },
                Err(err) => crate::logging::error(&format!("截图失败: {err}")),
            }
            finish(hwnd);
        }
        _ => {}
    }
}

const IDYES: i32 = 6;

/// 简易结果浮窗（MessageBox）；`confirm` 为 true 时带 是/否 按钮
unsafe fn message_box(parent: HWND, text: &str, title: &str, confirm: bool) -> i32 {
    let flags = windows::Win32::UI::WindowsAndMessaging::MB_TOPMOST
        | if confirm {
            windows::Win32::UI::WindowsAndMessaging::MB_YESNO
        } else {
            windows::Win32::UI::WindowsAndMessaging::MB_ICONINFORMATION
        };
    let text = windows::core::HSTRING::from(text);
    let title = windows::core::HSTRING::from(title);
    windows::Win32::UI::WindowsAndMessaging::MessageBoxW(
        parent,
        windows::core::PCWSTR(text.as_ptr()),
        windows::core::PCWSTR(title.as_ptr()),
        flags,
    )
    .0
}

extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                if let Some(st) = STATE.as_ref() {
                    let full = !st.first_paint_done;
                    let rc = if full {
                        RECT { left: 0, top: 0, right: st.screen.0, bottom: st.screen.1 }
                    } else {
                        ps.rcPaint
                    };
                    // 合成到离屏缓冲，再一次性上屏（消除中间状态闪烁）
                    paint(st.paint_dc, &rc);
                    let _ = BitBlt(
                        hdc,
                        rc.left, rc.top,
                        rc.right - rc.left, rc.bottom - rc.top,
                        st.paint_dc, rc.left, rc.top, SRCCOPY,
                    );
                    if full {
                        if let Some(st_mut) = STATE.as_mut() {
                            st_mut.first_paint_done = true;
                        }
                    }
                }
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            windows::Win32::UI::WindowsAndMessaging::WM_ERASEBKGND => {
                // 背景擦除交给离屏合成，避免闪烁
                LRESULT(1)
            }
            WM_LBUTTONDOWN => {
                let (x, y) = unpack_xy(lparam);
                if let Some(st) = STATE.as_mut() {
                    if st.annotating {
                        if let Some((lx, ly)) = local_point(st, x, y) {
                            use crate::annotate::Annotation;
                            let color = st.annot_color;
                            st.annot_preview = Some(match st.annot_tool {
                                AnnotTool::Pen => Annotation::Pen {
                                    points: vec![(lx, ly)],
                                    color,
                                    width: 3,
                                },
                                AnnotTool::Rect => Annotation::Rect {
                                    x0: lx,
                                    y0: ly,
                                    x1: lx,
                                    y1: ly,
                                    color,
                                    width: 2,
                                },
                            });
                            st.annot_dirty = preview_bounds(st);
                            if let Some(d) = st.annot_dirty {
                                let _ = InvalidateRect(st.hwnd, Some(&d.to_win32()), false);
                            }
                            let _ = SetCapture(hwnd);
                        }
                        return LRESULT(0);
                    }
                    if st.pending {
                        // 重新框选：收起工具条并清空上一轮标注
                        st.pending = false;
                        st.annotating = false;
                        st.annot_preview = None;
                        st.annot_dirty = None;
                        st.annotations = crate::annotate::AnnotationLayer::default();
                        toolbar::destroy(st.toolbar);
                        st.toolbar = HWND::default();
                        st.selection = None;
                        st.prev_border = None;
                    }
                    st.drag_start = Some((x, y));
                    st.cursor = (x, y);
                    let _ = SetCapture(hwnd);
                }
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                let (x, y) = unpack_xy(lparam);
                if let Some(st) = STATE.as_mut() {
                    st.cursor = (x, y);
                    if st.annotating && st.annot_preview.is_some() {
                        if let Some((lx, ly)) = local_point(st, x, y) {
                            use crate::annotate::Annotation;
                            if let Some(preview) = st.annot_preview.as_mut() {
                                match preview {
                                    Annotation::Pen { points, .. } => {
                                        if points.last() != Some(&(lx, ly)) {
                                            points.push((lx, ly));
                                        }
                                    }
                                    Annotation::Rect { x1, y1, .. } => {
                                        *x1 = lx;
                                        *y1 = ly;
                                    }
                                    _ => {}
                                }
                            }
                            let old = st.annot_dirty;
                            let new = preview_bounds(st);
                            let dirty = match (old, new) {
                                (Some(a), Some(b)) => Some(union(a, b)),
                                (a, b) => a.or(b),
                            };
                            if let Some(d) = dirty {
                                let _ = InvalidateRect(st.hwnd, Some(&d.to_win32()), false);
                            }
                            st.annot_dirty = new;
                        }
                        return LRESULT(0);
                    }
                    // 选区只在按住拖拽期间更新；pending 态（已松开）不跟随鼠标
                    if !st.pending {
                        if let Some((sx, sy)) = st.drag_start {
                            st.selection = Some(Rect {
                                x: sx.min(x),
                                y: sy.min(y),
                                width: (sx - x).abs().max(1),
                                height: (sy - y).abs().max(1),
                            });
                        }
                    }
                }
                redraw();
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                if let Some(st) = STATE.as_mut() {
                    if st.annotating {
                        if let Some(item) = st.annot_preview.take() {
                            st.annotations.add(item);
                        }
                        let _ = ReleaseCapture();
                        st.annot_dirty = None;
                        if let Some(sel) = st.selection {
                            let _ = InvalidateRect(st.hwnd, Some(&sel.to_win32()), false);
                        }
                        return LRESULT(0);
                    }
                }
                let confirmed = STATE
                    .as_ref()
                    .is_some_and(|st| st.drag_start.is_some() && st.selection.is_some());
                // 拖拽结束：清空 drag_start，选区从此刻定格
                if let Some(st) = STATE.as_mut() {
                    st.drag_start = None;
                }
                crate::logging::info(&format!("遮罩: 松开 confirmed={confirmed}"));
                if confirmed {
                    // 选区确认：进入 pending 模式，弹出工具条等待动作选择
                    let _ = ReleaseCapture();
                    if let Some(st) = STATE.as_mut() {
                        st.pending = true;
                        st.prev_border = st.selection.map(border_rect);
                    }
                    let sel_desktop = STATE.as_ref().map(|st| {
                        let sel = st.selection.unwrap();
                        Rect {
                            x: sel.x + st.origin.0,
                            y: sel.y + st.origin.1,
                            width: sel.width,
                            height: sel.height,
                        }
                    });
                    if let Some(sel_desktop) = sel_desktop {
                        match toolbar::show(hwnd, sel_desktop, toolbar::Kind::Main) {
                            Ok(tb) => {
                                crate::logging::info("遮罩: 工具条已弹出");
                                if let Some(st) = STATE.as_mut() {
                                    st.toolbar = tb;
                                }
                            }
                            Err(err) => crate::logging::error(&format!("工具条创建失败: {err}")),
                        }
                    }
                    // 选区从拖拽态定格，刷新边框高亮
                    let _ = InvalidateRect(hwnd, None, false);
                }
                LRESULT(0)
            }
            WM_RBUTTONUP | WM_KEYDOWN => {
                // 右键 / ESC 取消；标注模式下 ESC 先退回主工具条
                if msg == WM_KEYDOWN {
                    if wparam.0 as u32 != 0x1B {
                        return LRESULT(0);
                    }
                    if STATE.as_ref().is_some_and(|s| s.annotating) {
                        leave_annotate_mode(hwnd);
                        return LRESULT(0);
                    }
                }
                finish(hwnd);
                LRESULT(0)
            }
            WM_CHAR => {
                let ch = wparam.0 as u32;
                if ch == 0x0D {
                    // Enter 确认全屏选区
                    let pending = STATE.as_ref().map(|st| Rect {
                        x: st.origin.0,
                        y: st.origin.1,
                        width: st.screen.0,
                        height: st.screen.1,
                    });
                    *PENDING_SELECTION.lock().unwrap_or_else(|p| p.into_inner()) = pending;
                    let main = crate::app::main_hwnd();
                    finish(hwnd);
                    let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                        main,
                        SELECTION_DONE,
                        WPARAM(0),
                        LPARAM(0),
                    );
                } else if ch == b'c' as u32 || ch == b'C' as u32 {
                    // 颜色拾取：复制光标处 HEX（不截屏退出）
                    if let Some(st) = STATE.as_ref() {
                        let c = GetPixel(st.src_dc, st.cursor.0, st.cursor.1);
                        if c.0 != 0xFFFF_FFFF {
                            let (r, g, b) = (c.0 & 0xFF, (c.0 >> 8) & 0xFF, (c.0 >> 16) & 0xFF);
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
            crate::app::TOOLBAR_ACTION => {
                let action = wparam.0 as u32;
                crate::logging::info(&format!("遮罩: 收到工具条动作 {action}"));
                if action == toolbar::action::ANNOTATE {
                    enter_annotate_mode(hwnd);
                } else if !handle_annotate_action(hwnd, action) {
                    handle_toolbar_action(hwnd, action);
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                if let Some(st) = STATE.as_ref() {
                    cleanup_gdi(st);
                }
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
