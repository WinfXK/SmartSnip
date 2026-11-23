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

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
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
                    draw_text_5x7(frame, *x, *y, content, color.rgb());
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
            frame.data[i] = rgb[0];
            frame.data[i + 1] = rgb[1];
            frame.data[i + 2] = rgb[2];
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

/// 5x7 点阵文字（ASCII 可打印区；非 ASCII 以占位块渲染）
const FONT_5X7: &[[u8; 7]; 95] = &font_table();

const fn font_table() -> [[u8; 7]; 95] {
    // 仅登记常用 ASCII（' '~'~'），未覆盖字符渲染为实心块
    let mut t = [[0b11111; 7]; 95];
    t[0] = [0, 0, 0, 0, 0, 0, 0]; // ' '
    t[1] = [0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0, 0b00100]; // '!'
    t[33] = [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110]; // '8'
    t[65] = [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001]; // 'A'
    t[66] = [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110]; // 'B'
    t[67] = [0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110]; // 'C'
    t[83] = [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110]; // 'S'
    t
}

/// 5x7 点阵渲染；中文等宽字符按占位块处理（后续接入 GDI 文本绘制替换）
fn draw_text_5x7(frame: &mut Frame, x: i32, y: i32, text: &str, rgb: [u8; 3]) {
    let mut cx = x;
    for ch in text.chars() {
        let ascii = ch as u32;
        let glyph: [u8; 7] = if (0x20..=0x7E).contains(&ascii) {
            FONT_5X7[(ascii - 0x20) as usize]
        } else {
            [0b11111; 7]
        };
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..5 {
                if bits & (1 << (4 - col)) != 0 {
                    let (px, py) = (cx + col, y + row as i32);
                    if px >= 0 && py >= 0 {
                        draw_line_1px(frame, (px, py), (px, py), rgb);
                    }
                }
            }
        }
        cx += 6;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame8() -> Frame {
        Frame::new(8, 8)
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
        assert_eq!(f.data[(4 * 8) as usize * 4], 0xE5);
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
        assert_eq!(f.data[top], 0x1E); // Blue B
        let bottom = (6 * 8 + 6) as usize * 4;
        assert_eq!(f.data[bottom], 0x1E);
    }

    #[test]
    fn text_renders_placeholder_for_cjk() {
        let mut layer = AnnotationLayer::default();
        layer.add(Annotation::Text { x: 0, y: 0, content: "中".into(), color: AnnotateColor::Black });
        let mut f = frame8();
        layer.render(&mut f);
        assert_eq!(f.data[0], 0x21); // 黑色占位块
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
