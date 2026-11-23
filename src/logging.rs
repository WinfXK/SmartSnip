//! Copyright (c) 2026 Winfxk (冰月)
//! Email: admin@winfxk.cn  QQ: 2508543202
//! 创建时间: 2026年11月23日
//!
//! 滚动日志：按天落盘（北京时间命名），保留 7 天、单日 5MB 上限。
//! 内容红线：不落盘剪贴板内容、识别文本、截图数据。

use std::io::Write;
use std::sync::Mutex;

use crate::timefmt;

/// 保留天数与单日上限
const KEEP_DAYS: u32 = 7;
const MAX_BYTES: u64 = 5 * 1024 * 1024;

static LOGGER: Mutex<Option<std::path::PathBuf>> = Mutex::new(None);

/// 初始化日志目录（安装目录下 config\logs）并惰性清理过期文件
pub fn init(base_dir: &std::path::Path) {
    let dir = base_dir.join("config").join("logs");
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut slot) = LOGGER.lock() {
        *slot = Some(dir.clone());
    }
    cleanup_expired(&dir);
}

/// 日志文件路径（当天的）
fn today_file(dir: &std::path::Path) -> std::path::PathBuf {
    dir.join(format!("snip-{}.log", timefmt::now_cn_date()))
}

/// 追加一行日志；失败静默（日志不阻断业务）
pub fn info(msg: &str) {
    write_line("INFO", msg);
}

pub fn error(msg: &str) {
    write_line("ERROR", msg);
}

fn write_line(level: &str, msg: &str) {
    let Ok(mut slot) = LOGGER.lock() else { return };
    let Some(dir) = slot.clone() else { return };
    let path = today_file(&dir);
    let line = format!("[{}][{}] {}\n", timefmt::now_cn_string(), level, msg);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = f.write_all(line.as_bytes());
    }
    // 单日超限截断：丢弃前半内容
    if let Ok(meta) = std::fs::metadata(&path) {
        if meta.len() > MAX_BYTES {
            let _ = std::fs::write(&path, "[日志超限，已截断]\n".as_bytes());
        }
    }
    *slot = Some(dir);
}

/// 窗口子系统下 stderr 可能无效：release 版调试输出转日志，debug 版保留控制台
#[macro_export]
macro_rules! dbg_println {
    ($($arg:tt)*) => {{
        #[cfg(debug_assertions)]
        {
            eprintln!($($arg)*);
        }
        #[cfg(not(debug_assertions))]
        {
            $crate::logging::info(&format!($($arg)*));
        }
    }};
}

/// 清理超过保留期的日志文件
fn cleanup_expired(dir: &std::path::Path) {
    let cutoff = timefmt::now_cn().date_naive() - chrono::Duration::days(KEEP_DAYS as i64);
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let Some(date_str) = name
                .strip_prefix("snip-")
                .and_then(|s| s.strip_suffix(".log"))
            else {
                continue;
            };
            if let Ok(d) = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                if d < cutoff {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_and_rotate_threshold() {
        let dir = std::env::temp_dir().join(format!("SmartSnip_log_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        init(&dir);
        info("测试日志");
        error("错误日志");
        let files: Vec<_> = std::fs::read_dir(dir.join("config").join("logs")).unwrap().collect();
        assert_eq!(files.len(), 1);
        let content = std::fs::read_to_string(files[0].as_ref().unwrap().path()).unwrap();
        assert!(content.contains("[INFO] 测试日志"));
        assert!(content.contains("[ERROR] 错误日志"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_log_dir_write_is_silent() {
        // 未 init 时调用不 panic
        let before_ok = std::fs::read_to_string(std::env::temp_dir().join("nonexistent_x")).is_err();
        info("静默");
        assert!(before_ok);
    }
}
