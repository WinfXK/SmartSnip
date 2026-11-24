//! Copyright (c) 2026 Winfxk (冰月)
//! Email: admin@winfxk.cn  QQ: 2508543202
//! 创建时间: 2026年11月23日
//!
//! 二维码识别：rqrr 本地离线解码。
//! 内容分流：文本 → 直接剪贴板；URL/协议跳转 → 确认后打开；高危 scheme 附风险提示。

use rqrr::PreparedImage;

use crate::capture::Frame;

/// 二维码内容分类
#[derive(Debug, Clone, PartialEq)]
pub enum QrContent {
    Text(String),
    /// 需确认后跳转的内容（URL / 协议链接）
    Jump(String),
}

/// 高危协议：打开前附加醒目风险提示
const HIGH_RISK_SCHEMES: &[&str] = &["file:", "ms-msdt:", "search:", "ms-officecmd:", "ms-settings:"];

/// BGRA 帧转灰度图并解码；返回 None 表示未发现二维码
pub fn decode(frame: &Frame) -> Option<QrContent> {
    // rqrr 要求亮度图（Luma）；BGRA → 灰度
    let width = frame.width as usize;
    let height = frame.height as usize;
    let mut gray = image::GrayImage::new(frame.width, frame.height);
    for y in 0..height {
        for x in 0..width {
            let i = (y * width + x) * 4;
            let (b, g, r) = (frame.data[i] as u32, frame.data[i + 1] as u32, frame.data[i + 2] as u32);
            let l = ((r * 299 + g * 587 + b * 114) / 1000) as u8;
            gray.put_pixel(x as u32, y as u32, image::Luma([l]));
        }
    }
    let mut prepared = PreparedImage::prepare(gray);
    let grids = prepared.detect_grids();
    for grid in grids {
        if let Ok(content) = grid.decode() {
            return Some(classify(&content.1));
        }
    }
    None
}

/// 文本 / 跳转分流
pub fn classify(text: &str) -> QrContent {
    let t = text.trim_start();
    let looks_like_url = t.len() >= 3
        && (t.starts_with("http://")
            || t.starts_with("https://")
            || t.chars().take_while(|c| *c != ':').all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
                && t.contains(':'));
    if looks_like_url {
        QrContent::Jump(t.to_string())
    } else {
        QrContent::Text(t.to_string())
    }
}

/// 是否高危协议
pub fn is_high_risk(content: &str) -> bool {
    let lower = content.to_ascii_lowercase();
    HIGH_RISK_SCHEMES.iter().any(|s| lower.starts_with(s))
}

/// 确认后打开跳转目标（ShellExecute 走系统默认关联，避免 cmd 解析带来的命令注入）
pub fn open_jump(target: &str) -> std::io::Result<()> {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let op = HSTRING::from("open");
    let file = HSTRING::from(target);
    let ret = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(op.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if ret.0 as isize > 32 {
        Ok(())
    } else {
        Err(std::io::Error::other("ShellExecuteW 打开失败"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_plain_text() {
        assert_eq!(classify("hello world"), QrContent::Text("hello world".into()));
    }

    #[test]
    fn classify_https_is_jump() {
        assert_eq!(classify("https://example.com/x"), QrContent::Jump("https://example.com/x".into()));
    }

    #[test]
    fn classify_custom_scheme_is_jump() {
        assert!(matches!(classify("mqqapi://im/chat?chat=u"), QrContent::Jump(_)));
    }

    #[test]
    fn high_risk_detection() {
        assert!(is_high_risk("file:///C:/x.exe"));
        assert!(is_high_risk("MS-MSDT:/run"));
        assert!(!is_high_risk("https://example.com"));
    }

    #[test]
    fn no_qr_returns_none() {
        let f = Frame::new(64, 64);
        assert_eq!(decode(&f), None);
    }
}
