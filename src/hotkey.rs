//! Copyright (c) 2026 Winfxk (冰月)
//! Email: admin@winfxk.cn  QQ: 2508543202
//! 创建时间: 2026年11月23日
//!
//! 全局快捷键引擎：RegisterHotKey 系统级注册。
//! 冲突检测：注册失败即冲突；修改流程「先试注册，成功才落盘」。

use serde::{Deserialize, Serialize};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey,
    UnregisterHotKey,
};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::WM_HOTKEY;

use crate::app::HOTKEY_ID_SNIP;

/// 快捷键组合（配置持久化模型）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HotKeyCombo {
    pub modifiers: Vec<String>,
    pub key: String,
}

#[derive(Debug, thiserror::Error)]
pub enum HotKeyError {
    /// 组合已被其他程序占用
    #[error("快捷键被其他程序占用")]
    Conflict,
    #[error("无效的快捷键组合")]
    Invalid,
    #[error("系统错误: {0}")]
    System(windows::core::Error),
}

impl Default for HotKeyCombo {
    fn default() -> Self {
        Self {
            modifiers: vec!["ctrl".into(), "alt".into()],
            key: "s".into(),
        }
    }
}

/// 单个修饰键 → Win32 修饰符
fn modifier_flag(name: &str) -> Option<HOT_KEY_MODIFIERS> {
    match name.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => Some(MOD_CONTROL),
        "alt" => Some(MOD_ALT),
        "shift" => Some(MOD_SHIFT),
        "win" => Some(MOD_WIN),
        _ => None,
    }
}

/// 字符键 → 虚拟键码（覆盖 A~Z、0~9、F1~F12；其余键后续按需扩展）
fn key_code(name: &str) -> Option<u32> {
    let n = name.trim().to_ascii_lowercase();
    let bytes = n.as_bytes();
    match bytes {
        [b'a'..=b'z'] => Some(bytes[0].to_ascii_uppercase() as u32),
        [b'0'..=b'9'] => Some(bytes[0] as u32),
        _ if (2..=3).contains(&n.len()) && n.starts_with('f') => n[1..].parse::<u32>().ok().filter(|f| (1..=12).contains(f)).map(|f| 0x70 + f - 1),
        _ => None,
    }
}

impl HotKeyCombo {
    /// 校验并转换为 Win32 参数；要求至少一个修饰键
    pub fn to_win32(&self) -> Result<(HOT_KEY_MODIFIERS, u32), HotKeyError> {
        let mut mods = HOT_KEY_MODIFIERS(0);
        let mut has_modifier = false;
        for m in &self.modifiers {
            match modifier_flag(m) {
                Some(flag) => {
                    mods |= flag;
                    has_modifier = true;
                }
                None => return Err(HotKeyError::Invalid),
            }
        }
        if !has_modifier {
            return Err(HotKeyError::Invalid);
        }
        let key = key_code(&self.key).ok_or(HotKeyError::Invalid)?;
        Ok((mods, key))
    }
}

/// 已注册的快捷键句柄；注销在 Drop 中完成
/// HWND 原始指针本身非 Send/Sync，句柄仅在主窗口线程使用，安全标注收口于此
pub struct RegisteredHotKey {
    hwnd: HWND,
    id: u32,
}

unsafe impl Send for RegisteredHotKey {}
unsafe impl Sync for RegisteredHotKey {}

impl RegisteredHotKey {
    /// 注册成功返回句柄；失败区分冲突与无效
    pub fn register(hwnd: HWND, combo: &HotKeyCombo) -> Result<RegisteredHotKey, HotKeyError> {
        let (mods, key) = combo.to_win32()?;
        unsafe {
            RegisterHotKey(hwnd, HOTKEY_ID_SNIP as i32, mods | MOD_NOREPEAT, key)
                .map_err(|err| {
                    // ERROR_HOTKEY_ALREADY_REGISTERED(1409) → 冲突
                    if err.code() == windows::core::HRESULT(0x80070581u32 as i32) {
                        HotKeyError::Conflict
                    } else {
                        HotKeyError::System(err)
                    }
                })?;
        }
        Ok(RegisteredHotKey { hwnd, id: HOTKEY_ID_SNIP })
    }

    /// 换绑：新组合注册成功才注销旧的
    pub fn rebind(&mut self, combo: &HotKeyCombo) -> Result<(), HotKeyError> {
        let new = RegisteredHotKey::register(self.hwnd, combo)?;
        unsafe {
            let _ = UnregisterHotKey(self.hwnd, self.id as i32);
        }
        *self = new;
        Ok(())
    }
}

impl Drop for RegisteredHotKey {
    fn drop(&mut self) {
        unsafe {
            let _ = UnregisterHotKey(self.hwnd, self.id as i32);
        }
    }
}

/// WM_HOTKEY 消息分流；返回 Some 表示已消费
pub fn handle_wm_hotkey(wparam: WPARAM, _lparam: LPARAM) -> Option<()> {
    if wparam.0 as u32 == HOTKEY_ID_SNIP {
        Some(())
    } else {
        None
    }
}

// WM_HOTKEY 保留供 app.rs 分流使用
const _: u32 = WM_HOTKEY;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_combo_is_valid() {
        let (mods, key) = HotKeyCombo::default().to_win32().unwrap();
        assert!(mods.0 & MOD_CONTROL.0 != 0 && mods.0 & MOD_ALT.0 != 0);
        assert_eq!(key, b'S' as u32);
    }

    #[test]
    fn single_modifier_without_base_key_rejected() {
        let combo = HotKeyCombo { modifiers: vec![], key: "a".into() };
        assert!(matches!(combo.to_win32(), Err(HotKeyError::Invalid)));
    }

    #[test]
    fn unknown_modifier_rejected() {
        let combo = HotKeyCombo { modifiers: vec!["hyper".into()], key: "a".into() };
        assert!(matches!(combo.to_win32(), Err(HotKeyError::Invalid)));
    }

    #[test]
    fn f_keys_map_correctly() {
        let combo = HotKeyCombo { modifiers: vec!["ctrl".into()], key: "F1".into() };
        assert_eq!(combo.to_win32().unwrap().1, 0x70);
        let combo = HotKeyCombo { modifiers: vec!["ctrl".into()], key: "F13".into() };
        assert!(matches!(combo.to_win32(), Err(HotKeyError::Invalid)));
    }

    #[test]
    fn digits_map_correctly() {
        let combo = HotKeyCombo { modifiers: vec!["alt".into()], key: "9".into() };
        assert_eq!(combo.to_win32().unwrap().1, b'9' as u32);
    }
}
