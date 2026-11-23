//! Copyright (c) 2026 Winfxk (冰月)
//! Email: admin@winfxk.cn  QQ: 2508543202
//! 创建时间: 2026年11月23日
//!
//! 单实例守护：命名互斥体防多开；二次启动向既有实例的
//! 隐藏主窗口投递 `ACTIVATE` 消息（触发一次截图）后退出。

use windows::core::{Error, Result, HRESULT};
use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW};

use crate::app::{ACTIVATE_SNIP, MAIN_WINDOW_CLASS};

/// 全局互斥体名（Local\ 与用户会话绑定）
pub const MUTEX_NAME: &str = r"Local\SmartSnip_SingleInstance";

/// 尝试持有单实例；已有实例在运行时返回 Err 并完成唤醒动作
pub fn acquire() -> Result<SingleInstance> {
    unsafe {
        let name = windows::core::HSTRING::from(MUTEX_NAME);
        let handle = CreateMutexW(None, true, &name)?;
        let last = GetLastError();
        if last == ERROR_ALREADY_EXISTS {
            let _ = CloseHandle(handle);
            wake_existing();
            return Err(Error::new(HRESULT::from_win32(0x8004_0001), "已有 SmartSnip 实例在运行"));
        }
        Ok(SingleInstance { mutex: handle })
    }
}

/// 向既有实例主窗口投递截图激活消息
fn wake_existing() {
    unsafe {
        let class = windows::core::HSTRING::from(MAIN_WINDOW_CLASS);
        if let Ok(hwnd) = FindWindowW(windows::core::PCWSTR(class.as_ptr()), None) {
            if !hwnd.is_invalid() {
                let _ = PostMessageW(hwnd, ACTIVATE_SNIP, WPARAM(0), LPARAM(0));
            }
        }
    }
}

/// 持有期内保持互斥体句柄不释放
pub struct SingleInstance {
    mutex: windows::Win32::Foundation::HANDLE,
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.mutex);
        }
    }
}

#[allow(unused)]
fn _unused(hwnd: HWND) {}
