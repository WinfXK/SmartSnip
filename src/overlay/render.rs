//! Copyright (c) 2026 Winfxk (冰月)
//! Email: admin@winfxk.cn  QQ: 2508543202
//! 创建时间: 2026年11月23日
//!
//! 覆盖层软件光栅化：BGRA 缓冲绘制（压暗、选区高亮边框、放大镜）。

use crate::capture::{Frame, Rect};

/// 遮罩压暗色（半透明黑由源混合系数控制，此处为常量基色）
pub const DIM_COLOR: u32 = 0x00000000;
/// 选区边框色
pub const BORDER_COLOR: [u8; 3] = [0x00, 0xD7, 0xFF]; // 金黄色系
/// 放大镜尺寸（物理像素）
pub const MAG_SIZE: i32 = 120;
/// 放大倍率
pub const MAG_ZOOM: i32 = 8;

/// 在整帧上绘制遮罩：选区内保持原样，选区外每像素压暗（alpha*0.4）
pub fn draw_dim(frame: &mut Frame, selection: Rect) {
    let (w, h) = (frame.width as i32, frame.height as i32);
    for y in 0..h {
        let inside_y = y >= selection.y && y < selection.y + selection.height;
        let row = &mut frame.data[(y * w * 4) as usize..((y + 1) * w * 4) as usize];
        for x in 0..w {
            let inside_x = x >= selection.x && x < selection.x + selection.width;
            if inside_x && inside_y {
                continue;
            }
            let i = (x * 4) as usize;
            for c in row.iter_mut().skip(i).take(3) {
                *c = (*c as u32 * 6 / 10) as u8;
            }
        }
    }
}

/// 绘制选区边框（2px）
pub fn draw_border(frame: &mut Frame, selection: Rect) {
    let (w, h) = (frame.width as i32, frame.height as i32);
    let draw_hline = |frame: &mut Frame, y: i32, x0: i32, x1: i32| {
        if y < 0 || y >= h {
            return;
        }
        for x in x0.max(0)..x1.min(w) {
            let i = (y * w + x) as usize * 4;
            frame.data[i] = BORDER_COLOR[0];
            frame.data[i + 1] = BORDER_COLOR[1];
            frame.data[i + 2] = BORDER_COLOR[2];
        }
    };
    let draw_vline = |frame: &mut Frame, x: i32, y0: i32, y1: i32| {
        if x < 0 || x >= w {
            return;
        }
        for y in y0.max(0)..y1.min(h) {
            let i = (y * w + x) as usize * 4;
            frame.data[i] = BORDER_COLOR[0];
            frame.data[i + 1] = BORDER_COLOR[1];
            frame.data[i + 2] = BORDER_COLOR[2];
        }
    };
    for t in 0..2 {
        draw_hline(frame, selection.y + t, selection.x, selection.x + selection.width);
        draw_hline(frame, selection.y + selection.height - 1 - t, selection.x, selection.x + selection.width);
        draw_vline(frame, selection.x + t, selection.y, selection.y + selection.height);
        draw_vline(frame, selection.x + selection.width - 1 - t, selection.y, selection.y + selection.height);
    }
}

/// 放大镜输出：以光标为中心取样源帧（遮罩前），渲染到目标区域
pub fn draw_magnifier(src: &Frame, out: &mut Frame, cursor: (i32, i32), mag_origin: (i32, i32)) {
    let half = MAG_SIZE / 2;
    let (ox, oy) = mag_origin;
    for my in 0..MAG_SIZE {
        for mx in 0..MAG_SIZE {
            let sx = cursor.0 + (mx - half) / MAG_ZOOM;
            let sy = cursor.1 + (my - half) / MAG_ZOOM;
            let dst_x = ox + mx;
            let dst_y = oy + my;
            if dst_x < 0 || dst_x >= out.width as i32 || dst_y < 0 || dst_y >= out.height as i32 {
                continue;
            }
            let di = (dst_y * out.width as i32 + dst_x) as usize * 4;
            if sx >= 0 && sx < src.width as i32 && sy >= 0 && sy < src.height as i32 {
                let si = (sy * src.width as i32 + sx) as usize * 4;
                out.data[di] = src.data[si];
                out.data[di + 1] = src.data[si + 1];
                out.data[di + 2] = src.data[si + 2];
            } else {
                out.data[di..di + 3].fill(0x20);
            }
        }
    }
}

/// 取像素颜色（放大镜面板显示用）
pub fn pixel_color(frame: &Frame, x: i32, y: i32) -> Option<(u8, u8, u8)> {
    if x < 0 || x >= frame.width as i32 || y < 0 || y >= frame.height as i32 {
        return None;
    }
    let i = (y * frame.width as i32 + x) as usize * 4;
    Some((frame.data[i], frame.data[i + 1], frame.data[i + 2]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_frame(w: u32, h: u32, v: u8) -> Frame {
        let mut f = Frame::new(w, h);
        f.data.iter_mut().for_each(|b| *b = v);
        f
    }

    #[test]
    fn dim_darkens_outside_selection_only() {
        let mut f = make_frame(10, 10, 100);
        let sel = Rect { x: 3, y: 3, width: 4, height: 4 };
        draw_dim(&mut f, sel);
        // 选区内保持原值
        assert_eq!(f.data[(4 * 10 + 4) as usize * 4], 100);
        // 选区外压暗
        assert_eq!(f.data[0], 60);
    }

    #[test]
    fn border_draws_on_selection_edges() {
        let mut f = make_frame(10, 10, 0);
        let sel = Rect { x: 2, y: 2, width: 4, height: 4 };
        draw_border(&mut f, sel);
        let i = (2 * 10 + 2) as usize * 4;
        assert_eq!(&f.data[i..i + 3], &BORDER_COLOR);
    }

    #[test]
    fn magnifier_samples_center() {
        let src = make_frame(100, 100, 77);
        let mut out = make_frame(200, 200, 0);
        draw_magnifier(&src, &mut out, (50, 50), (10, 10));
        // 放大镜中心对应源像素 (50,50)
        let center = ((10 + MAG_SIZE / 2) * 200 + 10 + MAG_SIZE / 2) as usize * 4;
        assert_eq!(out.data[center], 77);
    }

    #[test]
    fn pixel_color_bounds() {
        let f = make_frame(2, 2, 5);
        assert_eq!(pixel_color(&f, 1, 1), Some((5, 5, 5)));
        assert_eq!(pixel_color(&f, 9, 9), None);
    }
}
