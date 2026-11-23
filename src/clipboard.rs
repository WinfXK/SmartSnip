//! Copyright (c) 2026 Winfxk (冰月)
//! Email: admin@winfxk.cn  QQ: 2508543202
//! 创建时间: 2026年11月23日
//!
//! 剪贴板：位图（DIB）与文本写入，基于 arboard；另存 PNG。

use crate::capture::Frame;

/// 位图写入剪贴板（arboard 写 DIB，IM/办公软件通用）
pub fn copy_frame(frame: &Frame) -> Result<(), String> {
    let width = frame.width as usize;
    let height = frame.height as usize;
    // BGRA → RGBA
    let mut rgba = vec![0u8; frame.data.len()];
    for (dst, src) in rgba.chunks_exact_mut(4).zip(frame.data.chunks_exact(4)) {
        dst[0] = src[2];
        dst[1] = src[1];
        dst[2] = src[0];
        dst[3] = src[3];
    }
    let mut board =
        arboard::Clipboard::new().map_err(|e| format!("剪贴板初始化失败: {e}"))?;
    let img = arboard::ImageData {
        width,
        height,
        bytes: rgba.into(),
    };
    board
        .set_image(img)
        .map_err(|e| format!("写入剪贴板失败: {e}"))
}

/// 文本写入剪贴板
pub fn copy_text(text: &str) -> Result<(), String> {
    let mut board =
        arboard::Clipboard::new().map_err(|e| format!("剪贴板初始化失败: {e}"))?;
    board
        .set_text(text.to_string())
        .map_err(|e| format!("写入剪贴板失败: {e}"))
}

/// 帧另存为 PNG
pub fn save_frame_png(frame: &Frame, path: &std::path::Path) -> Result<(), String> {
    let width = frame.width as usize;
    let height = frame.height as usize;
    let mut rgba = vec![0u8; frame.data.len()];
    for (dst, src) in rgba.chunks_exact_mut(4).zip(frame.data.chunks_exact(4)) {
        dst[0] = src[2];
        dst[1] = src[1];
        dst[2] = src[0];
        dst[3] = src[3];
    }
    image::save_buffer(
        path,
        &rgba,
        width as u32,
        height as u32,
        image::ColorType::Rgba8,
    )
    .map_err(|e| format!("保存失败: {e}"))
}

/// 默认另存路径：用户图片目录 + 北京时间文件名
pub fn default_save_path() -> std::path::PathBuf {
    let dir = dirs_user_pictures();
    let name = format!("SmartSnip_{}.png", crate::timefmt::now_cn().format("%Y%m%d_%H%M%S"));
    dir.join(name)
}

fn dirs_user_pictures() -> std::path::PathBuf {
    let user = std::env::var("USERPROFILE").unwrap_or_default();
    std::path::Path::new(&user).join("Pictures")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_path_format() {
        let p = default_save_path();
        let name = p.file_name().unwrap().to_string_lossy();
        assert!(name.starts_with("SmartSnip_") && name.ends_with(".png"));
        assert!(name.contains("Pictures") == false);
    }

    #[test]
    fn save_png_roundtrip() {
        let mut f = Frame::new(4, 4);
        f.data[0] = 255;
        let dir = std::env::temp_dir().join(format!("SmartSnip_png_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("t.png");
        save_frame_png(&f, &path).unwrap();
        assert!(path.exists());
        let img = image::open(&path).unwrap();
        assert_eq!((img.width(), img.height()), (4, 4));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
