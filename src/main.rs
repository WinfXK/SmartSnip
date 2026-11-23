//! SmartSnip — 轻量级 Windows 常驻截图工具
//!
//! Copyright (c) 2026 Winfxk (冰月)
//! Email: admin@winfxk.cn  QQ: 2508543202
//! 创建时间: 2026年11月23日

mod app;
mod autostart;
mod capture;
mod config;
mod hotkey;
mod single_instance;
mod timefmt;
mod tray;

fn main() {
    let _single = match single_instance::acquire() {
        Ok(instance) => instance,
        Err(_) => {
            // 已唤醒既有实例，静默退出
            return;
        }
    };
    if let Err(err) = app::run() {
        eprintln!("SmartSnip 启动失败: {err}");
        std::process::exit(1);
    }
}
