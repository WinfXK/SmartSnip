//! Copyright (c) 2026 Winfxk (冰月)
//! Email: admin@winfxk.cn  QQ: 2508543202
//! 创建时间: 2026年11月23日
//!
//! 开机自启：HKCU CurrentVersion\Run 注册项，指向当前 exe 路径。

use windows::core::{Error, Result};
use windows::Win32::Foundation::{ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegGetValueW, RegOpenKeyExW, RegSetValueExW,
    HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_OPEN_CREATE_OPTIONS, REG_SAM_FLAGS,
    REG_SZ, RRF_RT_REG_SZ,
};

/// Run 键下的值名
pub const RUN_VALUE_NAME: &str = "SmartSnip";

fn run_key_path() -> &'static str {
    r"Software\Microsoft\Windows\CurrentVersion\Run"
}

/// 当前 exe 路径（含 `--background` 静默启动参数）
fn command_value() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    Some(format!("\"{}\" --background", exe.display()))
}

fn check(err: WIN32_ERROR) -> Result<()> {
    if err == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(Error::from_win32())
    }
}

fn utf16_bytes(s: &str) -> Vec<u8> {
    let mut wide: Vec<u8> = s.encode_utf16().flat_map(|w| w.to_le_bytes()).collect();
    wide.extend_from_slice(&[0, 0]);
    wide
}

/// 是否已注册自启
pub fn is_enabled() -> bool {
    unsafe {
        let path = windows::core::HSTRING::from(run_key_path());
        let mut key = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, windows::core::PCWSTR(path.as_ptr()), 0, KEY_READ, &mut key) != ERROR_SUCCESS {
            return false;
        }
        let value = windows::core::HSTRING::from(RUN_VALUE_NAME);
        let mut buf = [0u8; 520];
        let mut size = buf.len() as u32;
        let ok = RegGetValueW(
            key,
            windows::core::PCWSTR::null(),
            windows::core::PCWSTR(value.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        ) == ERROR_SUCCESS;
        let _ = RegCloseKey(key);
        ok
    }
}

/// 写入自启项
pub fn enable() -> Result<()> {
    let cmd = command_value().ok_or_else(Error::from_win32)?;
    unsafe {
        let path = windows::core::HSTRING::from(run_key_path());
        let mut key = HKEY::default();
        check(RegCreateKeyExW(
            HKEY_CURRENT_USER,
            windows::core::PCWSTR(path.as_ptr()),
            0,
            windows::core::PCWSTR::null(),
            REG_OPEN_CREATE_OPTIONS(0),
            REG_SAM_FLAGS(KEY_SET_VALUE.0),
            None,
            &mut key,
            None,
        ))?;
        let value = windows::core::HSTRING::from(RUN_VALUE_NAME);
        let data = utf16_bytes(&cmd);
        check(RegSetValueExW(
            key,
            windows::core::PCWSTR(value.as_ptr()),
            0,
            REG_SZ,
            Some(&data),
        ))?;
        let _ = RegCloseKey(key);
    }
    Ok(())
}

/// 删除自启项；项不存在时同样视为成功
pub fn disable() -> Result<()> {
    unsafe {
        let path = windows::core::HSTRING::from(run_key_path());
        let mut key = HKEY::default();
        check(RegOpenKeyExW(HKEY_CURRENT_USER, windows::core::PCWSTR(path.as_ptr()), 0, KEY_SET_VALUE, &mut key))?;
        let value = windows::core::HSTRING::from(RUN_VALUE_NAME);
        RegDeleteValueW(key, windows::core::PCWSTR(value.as_ptr()));
        let _ = RegCloseKey(key);
    }
    Ok(())
}
