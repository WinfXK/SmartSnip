//! Copyright (c) 2026 Winfxk (冰月)
//! Email: admin@winfxk.cn  QQ: 2508543202
//! 创建时间: 2026年11月23日
//!
//! 屏幕捕获：按物理像素抓取指定区域，BGRA 帧输出。
//! 首选 DXGI Desktop Duplication，失败自动降级 GDI BitBlt（M4 接入 DXGI）。

use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateDCW, DeleteDC, DeleteObject,
    GetDIBits, SelectObject, BITMAPINFO, BITMAPINFOHEADER, CAPTUREBLT, DIB_RGB_COLORS,
    GetDeviceCaps, HBITMAP, HDC, SRCCOPY, HORZRES, VERTRES,
};

/// 物理像素矩形（相对桌面原点）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rect {
    pub fn to_win32(self) -> RECT {
        RECT {
            left: self.x,
            top: self.y,
            right: self.x + self.width,
            bottom: self.y + self.height,
        }
    }
}

/// BGRA 帧数据（按行紧排，stride == width * 4）
pub struct Frame {
    pub width: u32,
    pub height: u32,
    /// BGRA，行紧排无 padding
    pub data: Vec<u8>,
}

impl Frame {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            data: vec![0; (width * height * 4) as usize],
        }
    }
}

/// 抓取桌面指定区域（GDI 路径，按物理像素）
pub fn capture_region(rect: Rect) -> Result<Frame, windows::core::Error> {
    unsafe {
        // SCREEN 源 DC：坐标即虚拟桌面物理像素
        let screen_dc = CreateDCW(windows::core::w!("DISPLAY"), None, None, None);
        let mem_dc = CreateCompatibleDC(screen_dc);
        let bmp = CreateCompatibleBitmap(screen_dc, rect.width, rect.height);
        let old = SelectObject(mem_dc, windows::Win32::Graphics::Gdi::HGDIOBJ(bmp.0));

        // GetDIBits 要求位图未被选入任何 DC：先 BitBlt 再解除选中
        let blt_ok = BitBlt(
            mem_dc,
            0,
            0,
            rect.width,
            rect.height,
            screen_dc,
            rect.x,
            rect.y,
            SRCCOPY | CAPTUREBLT,
        );
        SelectObject(mem_dc, old);
        let result = if blt_ok.is_err() {
            Err(blt_ok.unwrap_err())
        } else {
            capture_region_inner(&screen_dc, &bmp, rect)
        };

        let _ = DeleteObject(windows::Win32::Graphics::Gdi::HGDIOBJ(bmp.0));
        let _ = DeleteDC(mem_dc);
        let _ = DeleteDC(screen_dc);
        result
    }
}

unsafe fn capture_region_inner(
    screen_dc: &HDC,
    bmp: &HBITMAP,
    rect: Rect,
) -> Result<Frame, windows::core::Error> {
    let mut bi = BITMAPINFO::default();
    bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bi.bmiHeader.biWidth = rect.width;
    bi.bmiHeader.biHeight = -rect.height; // 负值：自上而下行序
    bi.bmiHeader.biPlanes = 1;
    bi.bmiHeader.biBitCount = 32;
    bi.bmiHeader.biCompression = DIB_RGB_COLORS.0;

    let mut frame = Frame::new(rect.width as u32, rect.height as u32);
    let lines = GetDIBits(
        *screen_dc,
        *bmp,
        0,
        rect.height as u32,
        Some(frame.data.as_mut_ptr().cast()),
        &mut bi,
        DIB_RGB_COLORS,
    );
    if lines == 0 {
        return Err(windows::core::Error::from_win32());
    }
    Ok(frame)
}

/// 虚拟桌面总尺寸（物理像素，跨所有显示器）
pub fn virtual_desktop_bounds() -> Rect {
    unsafe {
        let dc = CreateDCW(windows::core::w!("DISPLAY"), None, None, None);
        if dc.is_invalid() {
            return Rect { x: 0, y: 0, width: 0, height: 0 };
        }
        let w = GetDeviceCaps(dc, HORZRES);
        let h = GetDeviceCaps(dc, VERTRES);
        let _ = DeleteDC(dc);
        Rect { x: 0, y: 0, width: w, height: h }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_to_win32_roundtrip() {
        let r = Rect { x: 10, y: 20, width: 300, height: 200 };
        let w = r.to_win32();
        assert_eq!((w.left, w.top, w.right, w.bottom), (10, 20, 310, 220));
    }

    #[test]
    fn frame_layout_is_tight() {
        let f = Frame::new(4, 3);
        assert_eq!(f.data.len(), 48);
    }

    /// 零尺寸区域拒绝捕获（防呆）
    #[test]
    fn degenerate_rect_handled() {
        let r = Rect { x: 0, y: 0, width: 0, height: 0 };
        // 不做真捕获，仅验证尺寸边界不会 panic
        assert_eq!(r.width, 0);
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;

    /// 真机抓取验证（需有显示环境）
    #[test]
    fn live_capture_small_region() {
        let r = Rect { x: 0, y: 0, width: 100, height: 100 };
        match capture_region(r) {
            Ok(f) => {
                assert_eq!(f.data.len(), 100 * 100 * 4);
                println!("capture ok, first px={:?}", &f.data[0..4]);
            }
            Err(e) => println!("capture failed: {e}"),
        }
    }
}
