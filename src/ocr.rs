//! Copyright (c) 2026 Winfxk (冰月)
//! Email: admin@winfxk.cn  QQ: 2508543202
//! 创建时间: 2026年11月23日
//!
//! 文字识别：Windows.Media.Ocr（系统内置引擎，零模型体积）。
//! BGRA 帧 → PNG → SoftwareBitmap → 识别；后台线程执行，调用方负责超时控制。

use windows::core::Result;
use windows::Storage::Streams::{InMemoryRandomAccessStream, DataWriter};
use windows::Graphics::Imaging::{BitmapDecoder, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine;

use crate::capture::Frame;

/// 识别结果行
#[derive(Debug, Clone, PartialEq)]
pub struct OcrLine {
    pub text: String,
}

/// 系统可用的 OCR 语言标签列表
pub fn available_languages() -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(langs) = OcrEngine::AvailableRecognizerLanguages() {
        for l in langs {
            if let Ok(tag) = l.LanguageTag() {
                out.push(tag.to_string());
            }
        }
    }
    out
}

/// 使用用户配置语言创建引擎；失败返回 None（触发语言包引导提示）
pub fn try_engine() -> Option<OcrEngine> {
    OcrEngine::TryCreateFromUserProfileLanguages().ok()
}

/// BGRA 帧编码为 PNG 字节（供解码器构造 SoftwareBitmap）
fn frame_to_png(frame: &Frame) -> Result<Vec<u8>> {
    let mut rgba = vec![0u8; frame.data.len()];
    for (dst, src) in rgba.chunks_exact_mut(4).zip(frame.data.chunks_exact(4)) {
        dst[0] = src[2];
        dst[1] = src[1];
        dst[2] = src[0];
        dst[3] = src[3];
    }
    let mut png = std::io::Cursor::new(Vec::with_capacity(rgba.len() / 2));
    image::write_buffer_with_format(
        &mut png,
        &rgba,
        frame.width,
        frame.height,
        image::ColorType::Rgba8,
        image::ImageFormat::Png,
    )
    .map_err(|e| windows::core::Error::new(
        windows::core::HRESULT(0x8000_4005u32 as i32),
        e.to_string(),
    ))?;
    Ok(png.into_inner())
}

/// 识别一帧；返回按行文本
pub fn recognize(frame: &Frame) -> std::result::Result<Vec<OcrLine>, String> {
    let engine = try_engine().ok_or_else(|| {
        "未找到可用的 OCR 语言包。请在 设置 → 时间和语言 → 语言 中安装对应语言包".to_string()
    })?;
    let png = frame_to_png(frame).map_err(|e| format!("图像编码失败: {e}"))?;
    unsafe {
        // WinRT 需 STA/MTA 无强约束（RecognizeAsync 后台执行），此处标准初始化
        let png_slice: &[u8] = &png;
        let stream = InMemoryRandomAccessStream::new().map_err(|e| e.to_string())?;
        let writer = DataWriter::CreateDataWriter(&stream)
            .map_err(|e| e.to_string())?;
        writer
            .WriteBytes(png_slice)
            .map_err(|e| e.to_string())?;
        writer.StoreAsync().map_err(|e| e.to_string())?.get().map_err(|e| e.to_string())?;
        writer.FlushAsync().map_err(|e| e.to_string())?.get().map_err(|e| e.to_string())?;
        writer.DetachStream().map_err(|e| e.to_string())?;
        writer.Close().map_err(|e| e.to_string())?;

        let decoder = BitmapDecoder::CreateWithIdAsync(
            BitmapDecoder::PngDecoderId().map_err(|e| e.to_string())?,
            &stream,
        )
        .map_err(|e| e.to_string())?
        .get()
        .map_err(|e| e.to_string())?;
        let bitmap: SoftwareBitmap = decoder
            .GetSoftwareBitmapAsync()
            .map_err(|e| e.to_string())?
            .get()
            .map_err(|e| e.to_string())?;

        let ocr_result = engine
            .RecognizeAsync(&bitmap)
            .map_err(|e| e.to_string())?
            .get()
            .map_err(|e| e.to_string())?;
        let mut lines = Vec::new();
        if let Ok(text) = ocr_result.Text() {
            let text_str: String = text.to_string();
            for l in text_str.split('\n') {
                let t = l.trim_end_matches('\r').trim().to_string();
                if !t.is_empty() {
                    lines.push(OcrLine { text: t });
                }
            }
        }
        bitmap.Close().ok();
        stream.Close().ok();
        Ok(lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_to_png_produces_valid_png() {
        let f = Frame::new(8, 8);
        let png = frame_to_png(&f).expect("encode");
        assert!(png.len() > 8);
        assert_eq!(&png[1..4], b"PNG");
    }

    #[test]
    fn available_languages_returns_without_panic() {
        // 不假设系统装了哪些语言，只验证 API 可调
        let _ = available_languages();
    }
}
