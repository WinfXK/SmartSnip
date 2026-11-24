//! Copyright (c) 2026 Winfxk (冰月)
//! Email: admin@winfxk.cn  QQ: 2508543202
//! 创建时间: 2026年11月23日
//!
//! 覆盖层渲染辅助：放大镜参数、颜色取样。
//! 压暗改为按脏区 AlphaBlend（见 mod.rs WM_PAINT）。

use crate::capture::{Frame, Rect};

/// 选区边框色
pub const BORDER_COLOR: [u8; 3] = [0x00, 0xD7, 0xFF];
/// 放大镜尺寸（物理像素）
pub const MAG_SIZE: i32 = 120;
/// 放大倍率
pub const MAG_ZOOM: i32 = 8;
/// 放大镜在窗口内的位置
pub const MAG_ORIGIN: (i32, i32) = (8, 8);
/// 边框厚度
pub const BORDER_WIDTH: i32 = 2;

/// 放大镜取样的源区域（以光标为中心）
pub fn mag_source_rect(cursor: (i32, i32), frame_w: i32, frame_h: i32) -> Rect {
    let half = MAG_SIZE / 2 / MAG_ZOOM;
    Rect {
        x: (cursor.0 - half).max(0),
        y: (cursor.1 - half).max(0),
        width: (half * 2).min(frame_w),
        height: (half * 2).min(frame_h),
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
    fn mag_source_rect_clamps() {
        let r = mag_source_rect((2, 2), 100, 100);
        assert_eq!((r.x, r.y), (0, 0));
        let r = mag_source_rect((50, 50), 100, 100);
        assert_eq!(r.x, 50 - MAG_SIZE / 2 / MAG_ZOOM);
        assert!(r.width > 0);
    }

    #[test]
    fn pixel_color_bounds() {
        let f = make_frame(2, 2, 5);
        assert_eq!(pixel_color(&f, 1, 1), Some((5, 5, 5)));
        assert_eq!(pixel_color(&f, 9, 9), None);
    }
}
