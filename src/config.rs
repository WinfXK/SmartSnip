//! Copyright (c) 2026 Winfxk (冰月)
//! Email: admin@winfxk.cn  QQ: 2508543202
//! 创建时间: 2026年11月23日
//!
//! 配置持久化：TOML + 原子写（临时文件 + rename）。
//! 损坏时备份为 .bak 并以默认值重建，不阻断启动。

use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::timefmt;

/// 全局配置数据（TOML 直接映射）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub hotkey: HotKeyConfig,
    pub pin: PinConfig,
    pub annotate: AnnotateConfig,
    pub autosave_dir: String,
    pub autostart: bool,
    pub log_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HotKeyConfig {
    pub modifiers: Vec<String>,
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PinConfig {
    /// 存活贴图上限
    pub max_count: u32,
    /// 初始不透明度百分比（40~100）
    pub opacity: u8,
    /// 失焦后自动降低透明度
    pub blur_on_unfocus: bool,
    /// 失焦淡化目标透明度
    pub blur_opacity: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnnotateConfig {
    /// 默认标注色（HEX），取值 red/yellow/blue/black
    pub color: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey: HotKeyConfig {
                modifiers: vec!["ctrl".into(), "alt".into()],
                key: "s".into(),
            },
            pin: PinConfig {
                max_count: 20,
                opacity: 100,
                blur_on_unfocus: true,
                blur_opacity: 70,
            },
            annotate: AnnotateConfig {
                color: "red".into(),
            },
            autosave_dir: String::new(),
            autostart: false,
            log_level: "info".into(),
        }
    }
}

/// 进程内生效配置（启动时加载、设置保存后更新）；读取失败回退默认值
static RUNTIME: Mutex<Option<Config>> = Mutex::new(None);

/// 覆盖进程内生效配置
pub fn set_runtime(cfg: Config) {
    *RUNTIME.lock().unwrap_or_else(|p| p.into_inner()) = Some(cfg);
}

/// 读取当前生效配置（未初始化时返回默认值）
pub fn runtime() -> Config {
    RUNTIME
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone()
        .unwrap_or_default()
}

/// 配置存储句柄：负责定位目录与读写
pub struct ConfigStore {
    dir: PathBuf,
    /// 写锁串行化保存，避免并发原子写竞态
    lock: Mutex<()>,
}

impl ConfigStore {
    /// 标准安装目录 `%USERPROFILE%\Winfxk\{exe 名}`（exe 名取自自身路径）
    pub fn standard_dir() -> PathBuf {
        let exe = std::env::current_exe().ok();
        let profile = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("USERNAME").map(|user| format!(r"C:\Users\{user}")))
            .unwrap_or_default();
        let mut base = PathBuf::from(profile);
        base.push("Winfxk");
        let app_name = exe
            .as_ref()
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "SmartSnip".into());
        base.push(app_name);
        base
    }

    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            lock: Mutex::new(()),
        }
    }

    pub fn config_file(&self) -> PathBuf {
        self.dir.join("config").join("config.toml")
    }

    pub fn load(&self) -> Config {
        let path = self.config_file();
        match std::fs::read_to_string(&path) {
            Ok(text) => match toml::from_str::<Config>(&text) {
                Ok(cfg) => cfg,
                Err(err) => {
                    crate::dbg_println!("配置解析失败({err})，已备份并重建默认配置");
                    self.backup_broken(&path);
                    self.save(&Config::default());
                    Config::default()
                }
            },
            Err(_) => {
                let cfg = Config::default();
                self.save(&cfg);
                cfg
            }
        }
    }

    /// 原子写：先写同目录临时文件，再 rename 覆盖
    pub fn save(&self, cfg: &Config) {
        let _guard = self.lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let path = self.config_file();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let text = match toml::to_string_pretty(cfg) {
            Ok(t) => t,
            Err(_) => return,
        };
        let tmp = path.with_extension(format!("toml.tmp.{}", timefmt::now_cn().timestamp_millis()));
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }

    fn backup_broken(&self, path: &std::path::Path) {
        let bak = path.with_extension("toml.bak");
        let _ = std::fs::rename(path, &bak);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store(tag: &str) -> ConfigStore {
        let dir = std::env::temp_dir().join(format!("SmartSnip_test_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        ConfigStore::new(dir)
    }

    #[test]
    fn roundtrip_preserves_fields() {
        let store = temp_store("roundtrip");
        let mut cfg = Config::default();
        cfg.pin.max_count = 5;
        cfg.hotkey.key = "x".into();
        store.save(&cfg);
        let loaded = store.load();
        assert_eq!(loaded, cfg);
        let _ = std::fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn broken_config_recovers_to_default() {
        let store = temp_store("broken");
        let _ = std::fs::create_dir_all(store.config_file().parent().unwrap());
        std::fs::write(store.config_file(), "not [valid toml ===").unwrap();
        let cfg = store.load();
        assert_eq!(cfg, Config::default());
        // 坏文件已备份为 .bak
        assert!(store
            .config_file()
            .with_extension("toml.bak")
            .exists());
        let _ = std::fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn missing_config_creates_default_file() {
        let store = temp_store("missing");
        let cfg = store.load();
        assert_eq!(cfg, Config::default());
        assert!(store.config_file().exists());
        let _ = std::fs::remove_dir_all(&store.dir);
    }
}
