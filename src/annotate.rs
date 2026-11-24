//! Copyright (c) 2026 Winfxk (冰月)
//! Email: admin@winfxk.cn  QQ: 2508543202
//! 创建时间: 2026年11月23日
//!
//! 标注层：画笔 / 矩形 / 文字三类标注，四色可选，一步撤销。
//! 坐标为帧内物理像素；渲染按顺序叠加到 BGRA 帧。

use crate::capture::Frame;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotateColor {
    Red,
    Yellow,
    Blue,
    Black,
}

impl AnnotateColor {
    pub fn rgb(self) -> [u8; 3] {
        match self {
            Self::Red => [0xE5, 0x39, 0x35],
            Self::Yellow => [0xFF, 0xEB, 0x3B],
            Self::Blue => [0x1E, 0x88, 0xE5],
            Self::Black => [0x21, 0x21, 0x21],
        }
    }

    /// 配置名 → 颜色（未知值回退红色）
    pub fn from_name(name: &str) -> Self {
        match name.to_ascii_lowercase().as_str() {
            "yellow" => Self::Yellow,
            "blue" => Self::Blue,
            "black" => Self::Black,
            _ => Self::Red,
        }
    }

    /// 四色轮换
    pub fn next(self) -> Self {
        match self {
            Self::Red => Self::Yellow,
            Self::Yellow => Self::Blue,
            Self::Blue => Self::Black,
            Self::Black => Self::Red,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Annotation {
    /// 画笔：离散点折线（物理像素）
    Pen {
        points: Vec<(i32, i32)>,
        color: AnnotateColor,
        width: i32,
    },
    /// 矩形：左上/右下两角
    Rect {
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
        color: AnnotateColor,
        width: i32,
    },
    /// 文字：基点 + 内容（默认 16px）
    Text {
        x: i32,
        y: i32,
        content: String,
        color: AnnotateColor,
    },
}

/// 标注层：有序叠加，undo 弹出最后一项
#[derive(Debug, Default, Clone)]
pub struct AnnotationLayer {
    items: Vec<Annotation>,
}

impl AnnotationLayer {
    pub fn add(&mut self, a: Annotation) {
        self.items.push(a);
    }

    /// 撤销上一步；空层返回 false
    pub fn undo(&mut self) -> bool {
        self.items.pop().is_some()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// 按叠加顺序遍历标注（供屏幕实时绘制）
    pub fn items(&self) -> &[Annotation] {
        &self.items
    }

    /// 全量渲染到帧
    pub fn render(&self, frame: &mut Frame) {
        for a in &self.items {
            match a {
                Annotation::Pen { points, color, width } => {
                    for w in points.windows(2) {
                        draw_line(frame, w[0], w[1], color.rgb(), *width);
                    }
                }
                Annotation::Rect { x0, y0, x1, y1, color, width } => {
                    let left = (*x0).min(*x1);
                    let right = (*x0).max(*x1);
                    let top = (*y0).min(*y1);
                    let bottom = (*y0).max(*y1);
                    for t in 0..*width {
                        draw_line(frame, (left, top + t), (right, top + t), color.rgb(), 1);
                        draw_line(frame, (left, bottom - t), (right, bottom - t), color.rgb(), 1);
                        draw_line(frame, (left + t, top), (left + t, bottom), color.rgb(), 1);
                        draw_line(frame, (right - t, top), (right - t, bottom), color.rgb(), 1);
                    }
                }
                Annotation::Text { x, y, content, color } => {
                    render_text(frame, *x, *y, content, color.rgb());
                }
            }
        }
    }
}

/// Bresenham 直线（带粗细：中心线 + 逐方向偏移近似）
fn draw_line(frame: &mut Frame, a: (i32, i32), b: (i32, i32), rgb: [u8; 3], width: i32) {
    let w = width.max(1);
    let half = w / 2;
    for dy in -half..=half {
        for dx in -half..=half {
            draw_line_1px(frame, (a.0 + dx, a.1 + dy), (b.0 + dx, b.1 + dy), rgb);
        }
    }
}

fn draw_line_1px(frame: &mut Frame, a: (i32, i32), b: (i32, i32), rgb: [u8; 3]) {
    let w = frame.width as i32;
    let h = frame.height as i32;
    let put = |frame: &mut Frame, x: i32, y: i32| {
        if x >= 0 && x < w && y >= 0 && y < h {
            let i = (y * w + x) as usize * 4;
            // Frame 为 BGRA，按 B/G/R 写入
            frame.data[i] = rgb[2];
            frame.data[i + 1] = rgb[1];
            frame.data[i + 2] = rgb[0];
        }
    };
    let dx = (b.0 - a.0).abs();
    let dy = (b.1 - a.1).abs();
    let (sx, sy) = (
        if a.0 < b.0 { 1 } else { -1 },
        if a.1 < b.1 { 1 } else { -1 },
    );
    let mut err = dx - dy;
    let (mut x, mut y) = (a.0, a.1);
    loop {
        put(frame, x, y);
        if x == b.0 && y == b.1 {
            break;
        }
        let e2 = err * 2;
        if e2 > -dy {
            err -= dy;
            x += sx;
        }
        if e2 < dx {
            err += dx;
            y += sy;
        }
    }
}

/// 用 GDI 将文字绘制到 BGRA 帧（支持中文，点阵占位方案已替换）
pub(crate) fn render_text(frame: &mut Frame, x: i32, y: i32, text: &str, rgb: [u8; 3]) {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Foundation::COLORREF;
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, CreateDCW, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject,
        SelectObject, SetBkMode, SetTextColor, TextOutW, BITMAPINFO, BITMAPINFOHEADER,
        CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DIB_RGB_COLORS, FF_DONTCARE,
        FW_NORMAL, HGDIOBJ, OUT_DEFAULT_PRECIS, TRANSPARENT,
    };

    if text.is_empty() {
        return;
    }
    unsafe {
        let screen = CreateDCW(windows::core::w!("DISPLAY"), None, None, None);
        if screen.is_invalid() {
            return;
        }
        let mut bi = BITMAPINFO::default();
        bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bi.bmiHeader.biWidth = frame.width as i32;
        bi.bmiHeader.biHeight = -(frame.height as i32);
        bi.bmiHeader.biPlanes = 1;
        bi.bmiHeader.biBitCount = 32;
        bi.bmiHeader.biCompression = DIB_RGB_COLORS.0;
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let bmp = CreateDIBSection(screen, &bi, DIB_RGB_COLORS, &mut bits, None, 0).unwrap_or_default();
        if bits.is_null() {
            let _ = DeleteDC(screen);
            return;
        }
        std::ptr::copy_nonoverlapping(frame.data.as_ptr(), bits as *mut u8, frame.data.len());
        let dc = CreateCompatibleDC(screen);
        let old_bmp = SelectObject(dc, HGDIOBJ(bmp.0));
        let face = HSTRING::from("Microsoft YaHei UI");
        let font = CreateFontW(
            16,
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
            PCWSTR(face.as_ptr()),
        );
        let old_font = SelectObject(dc, HGDIOBJ(font.0));
        let _ = SetBkMode(dc, TRANSPARENT);
        let _ = SetTextColor(dc, COLORREF(rgb[0] as u32 | (rgb[1] as u32) << 8 | (rgb[2] as u32) << 16));
        let wide: Vec<u16> = text.encode_utf16().collect();
        let _ = TextOutW(dc, x, y, &wide);
        SelectObject(dc, old_font);
        let _ = DeleteObject(HGDIOBJ(font.0));
        SelectObject(dc, old_bmp);
        std::ptr::copy_nonoverlapping(bits as *const u8, frame.data.as_mut_ptr(), frame.data.len());
        let _ = DeleteObject(HGDIOBJ(bmp.0));
        let _ = DeleteDC(dc);
        let _ = DeleteDC(screen);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame8() -> Frame {
        Frame::new(8, 8)
    }

    #[test]
    fn color_names_and_cycle() {
        assert_eq!(AnnotateColor::from_name("yellow"), AnnotateColor::Yellow);
        assert_eq!(AnnotateColor::from_name("unknown"), AnnotateColor::Red);
        assert_eq!(AnnotateColor::Red.next(), AnnotateColor::Yellow);
        assert_eq!(AnnotateColor::Black.next(), AnnotateColor::Red);
    }

    #[test]
    fn undo_pops_last_item() {
        let mut layer = AnnotationLayer::default();
        layer.add(Annotation::Text { x: 0, y: 0, content: "A".into(), color: AnnotateColor::Red });
        layer.add(Annotation::Text { x: 1, y: 1, content: "B".into(), color: AnnotateColor::Blue });
        assert_eq!(layer.len(), 2);
        assert!(layer.undo());
        assert_eq!(layer.len(), 1);
        assert!(layer.undo());
        assert!(layer.is_empty());
        assert!(!layer.undo());
    }

    #[test]
    fn pen_renders_colored_pixels() {
        let mut layer = AnnotationLayer::default();
        layer.add(Annotation::Pen {
            points: vec![(0, 4), (7, 4)],
            color: AnnotateColor::Red,
            width: 1,
        });
        let mut f = frame8();
        layer.render(&mut f);
        // BGRA：红色 B=0x35
        assert_eq!(f.data[(4 * 8) as usize * 4], 0x35);
    }

    #[test]
    fn rect_renders_border() {
        let mut layer = AnnotationLayer::default();
        layer.add(Annotation::Rect {
            x0: 1, y0: 1, x1: 6, y1: 6,
            color: AnnotateColor::Blue,
            width: 1,
        });
        let mut f = frame8();
        layer.render(&mut f);
        let top = (1 * 8 + 1) as usize * 4;
        assert_eq!(f.data[top], 0xE5); // Blue 的 B 通道
        let bottom = (6 * 8 + 6) as usize * 4;
        assert_eq!(f.data[bottom], 0xE5);
    }

    #[test]
    fn text_renders_colored_pixels() {
        let mut layer = AnnotationLayer::default();
        layer.add(Annotation::Text { x: 2, y: 2, content: "Hi".into(), color: AnnotateColor::Red });
        let mut f = Frame::new(120, 40);
        layer.render(&mut f);
        // 文字像素应写入非零 RGB
        assert!(f.data.chunks_exact(4).any(|p| p[0] != 0 || p[1] != 0 || p[2] != 0));
    }

    #[test]
    fn drawing_clips_to_bounds() {
        let mut layer = AnnotationLayer::default();
        layer.add(Annotation::Pen {
            points: vec![(-5, 0), (100, 100)],
            color: AnnotateColor::Red,
            width: 1,
        });
        let mut f = frame8();
        layer.render(&mut f); // 不 panic 即通过
    }
}
