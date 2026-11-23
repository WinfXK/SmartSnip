//! Copyright (c) 2026 Winfxk (冰月)
//! Email: admin@winfxk.cn  QQ: 2508543202
//! 创建时间: 2026年11月23日
//!
//! 时间格式化统一走北京时间（UTC+8），不依赖系统本地时区。

use chrono::{DateTime, FixedOffset, Utc};

/// 北京时间固定偏移 UTC+8
const CN_OFFSET_HOURS: i32 = 8;

pub fn now_cn() -> DateTime<FixedOffset> {
    Utc::now().with_timezone(&FixedOffset::east_opt(CN_OFFSET_HOURS * 3600).unwrap())
}

/// `yyyy-MM-dd HH:mm:ss`（北京时间）
pub fn now_cn_string() -> String {
    now_cn().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// `yyyy-MM-dd`（北京时间，用于日志文件名）
pub fn now_cn_date() -> String {
    now_cn().format("%Y-%m-%d").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beijing_offset_is_plus_8() {
        assert_eq!(now_cn().offset().local_minus_utc(), 8 * 3600);
    }

    #[test]
    fn date_format_is_iso() {
        assert_eq!(now_cn_date().len(), 10);
        assert!(now_cn_string().contains(' '));
    }
}
