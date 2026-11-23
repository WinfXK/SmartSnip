//! Copyright (c) 2026 Winfxk (冰月)
//! Email: admin@winfxk.cn  QQ: 2508543202
//! 创建时间: 2026年11月23日
//!
//! 调度与容错：panic 落盘不崩进程、OCR 硬超时看护、限流合并。

use std::sync::mpsc;
use std::time::Duration;

/// 耗时任务硬超时
pub const LONG_TASK_TIMEOUT: Duration = Duration::from_secs(10);

/// 全局 panic hook：落盘后返回，进程继续运行（消息循环层面保活）
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let msg = format!("panic: {info}");
        crate::logging::error(&msg);
        crate::dbg_println!("{msg}");
    }));
}

/// 在独立线程执行 `f` 并施加硬超时；超时返回 None（不阻塞调用方）
pub fn with_timeout<T: Send + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
    timeout: Duration,
) -> Option<T> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(timeout).ok()
}

/// 简单限流器：窗口期内的重复请求直接拒绝
pub struct RateLimiter {
    min_interval: Duration,
    last: std::sync::Mutex<Option<std::time::Instant>>,
}

impl RateLimiter {
    pub fn new(min_interval: Duration) -> Self {
        Self {
            min_interval,
            last: std::sync::Mutex::new(None),
        }
    }

    /// 允许通过返回 true 并记录时间；窗口内返回 false
    pub fn allow(&self) -> bool {
        let mut last = self.last.lock().unwrap_or_else(|p| p.into_inner());
        match *last {
            Some(t) if t.elapsed() < self.min_interval => false,
            _ => {
                *last = Some(std::time::Instant::now());
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_returns_none_on_slow_task() {
        let r = with_timeout(|| std::thread::sleep(Duration::from_millis(300)), Duration::from_millis(50));
        assert!(r.is_none());
    }

    #[test]
    fn fast_task_returns_value() {
        let r = with_timeout(|| 42, Duration::from_secs(1));
        assert_eq!(r, Some(42));
    }

    #[test]
    fn rate_limiter_blocks_within_window() {
        let limiter = RateLimiter::new(Duration::from_millis(200));
        assert!(limiter.allow());
        assert!(!limiter.allow());
        std::thread::sleep(Duration::from_millis(220));
        assert!(limiter.allow());
    }
}
