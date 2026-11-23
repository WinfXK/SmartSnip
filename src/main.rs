//! SmartSnip — 轻量级 Windows 常驻截图工具
//!
//! Copyright (c) 2026 Winfxk (冰月)
//! Email: admin@winfxk.cn  QQ: 2508543202
//! 创建时间: 2026年11月23日

// release 版为窗口子系统：无控制台黑框；debug 保留控制台便于定位
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod annotate;
mod autostart;
mod capture;
mod clipboard;
mod config;
mod hotkey;
mod ocr;
mod overlay;
mod pin;
mod qrcode;
mod logging;
mod scheduler;
mod single_instance;
mod timefmt;
mod tray;
mod ui;

fn main() {
    scheduler::install_panic_hook();
    logging::init(&config::ConfigStore::standard_dir());
    let _single = match single_instance::acquire() {
        Ok(instance) => instance,
        Err(_) => {
            // 已唤醒既有实例，静默退出
            return;
        }
    };
    if let Err(err) = app::run() {
        crate::dbg_println!("SmartSnip 启动失败: {err}");
        std::process::exit(1);
    }
}
