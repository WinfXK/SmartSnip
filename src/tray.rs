//! Copyright (c) 2026 Winfxk (冰月)
//! Email: admin@winfxk.cn  QQ: 2508543202
//! 创建时间: 2026年11月23日
//!
//! 系统托盘：Shell_NotifyIcon + 原生弹出菜单。
//! 左键单击 → 立即截图；右键 → 菜单。事件经主窗口消息驱动，零轮询。

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::UI::Shell::{
    NOTIFYICONDATAW, Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIM_ADD,
    NIM_DELETE, NIM_MODIFY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, PostMessageW, SetForegroundWindow,
    TrackPopupMenu, HMENU, MF_CHECKED, MF_STRING, MENU_ITEM_FLAGS, TPM_RIGHTBUTTON, WM_CLOSE,
    WM_COMMAND, WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP,
};

use crate::app::{ACTIVATE_SNIP, TRAY_CALLBACK};

/// 菜单项 ID（WM_COMMAND WPARAM 低位）
pub mod menu {
    pub const SNIP_NOW: u16 = 1;
    pub const SETTINGS: u16 = 2;
    pub const OPEN_CONFIG_DIR: u16 = 3;
    pub const AUTOSTART_TOGGLE: u16 = 4;
    pub const EXIT: u16 = 5;
}

static mut NOTIFY_ICON: Option<NOTIFYICONDATAW> = None;

/// 创建托盘图标并挂接回调消息
pub fn init(hwnd: HWND) -> Result<(), windows::core::Error> {
    unsafe {
        let icon = crate::icon::app_icon();
        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            uCallbackMessage: TRAY_CALLBACK,
            hIcon: icon,
            ..Default::default()
        };
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        for (i, c) in "SmartSnip".encode_utf16().take(127).enumerate() {
            nid.szTip[i] = c;
        }
        if !Shell_NotifyIconW(NIM_ADD, &nid).as_bool() {
            let err = windows::core::Error::from_win32();
            crate::logging::error(&format!("托盘图标注册失败: {err}"));
            return Err(err);
        }
        // v0 回调语义：lparam 即鼠标消息（兼容性最好）
        NOTIFY_ICON = Some(nid);
    }
    Ok(())
}

/// 弹出托盘气泡提示（错误/引导信息，不刷屏由调用方控制）
pub fn notify(title: &str, message: &str) {
    unsafe {
        let Some(mut nid) = NOTIFY_ICON else { return };
        nid.uFlags = NIF_INFO;
        for (slot, ch) in nid.szInfoTitle.iter_mut().zip(title.encode_utf16().take(63)) {
            *slot = ch;
        }
        for (slot, ch) in nid.szInfo.iter_mut().zip(message.encode_utf16().take(255)) {
            *slot = ch;
        }
        let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
    }
}

/// 移除托盘图标（退出前调用）
pub fn shutdown() {
    unsafe {
        if let Some(mut nid) = NOTIFY_ICON.take() {
            let _ = Shell_NotifyIconW(NIM_DELETE, &mut nid);
        }
    }
}

/// 右键弹出菜单；自启项勾选态跟随注册表
pub fn show_context_menu(hwnd: HWND) {
    unsafe {
        let menu: HMENU = CreatePopupMenu().unwrap_or_default();
        if menu.is_invalid() {
            return;
        }
        let _ = AppendMenuW(menu, MENU_ITEM_FLAGS(MF_STRING.0), menu::SNIP_NOW as usize, windows::core::w!("立即截图"));
        let _ = AppendMenuW(menu, MENU_ITEM_FLAGS(MF_STRING.0), menu::SETTINGS as usize, windows::core::w!("修改快捷键"));
        let _ = AppendMenuW(menu, MENU_ITEM_FLAGS(MF_STRING.0), menu::OPEN_CONFIG_DIR as usize, windows::core::w!("打开配置目录"));
        let auto_flag = if crate::autostart::is_enabled() {
            MENU_ITEM_FLAGS(MF_CHECKED.0 | MF_STRING.0)
        } else {
            MENU_ITEM_FLAGS(MF_STRING.0)
        };
        let _ = AppendMenuW(menu, auto_flag, menu::AUTOSTART_TOGGLE as usize, windows::core::w!("开机自启"));
        let _ = AppendMenuW(menu, MENU_ITEM_FLAGS(MF_STRING.0), menu::EXIT as usize, windows::core::w!("退出"));

        // 规避已知焦点问题：前置前台后弹出，菜单关闭后补发 WM_NULL
        let _ = SetForegroundWindow(hwnd);
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let _ = TrackPopupMenu(menu, TPM_RIGHTBUTTON, pt.x, pt.y, 0, hwnd, None);
        let _ = PostMessageW(hwnd, WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
    }
}

/// 存在存活贴图时的退出确认
fn confirm_exit(hwnd: HWND) -> bool {
    unsafe {
        let text = windows::core::HSTRING::from(format!(
            "当前还有 {} 张贴图，退出将一并关闭。确定退出？",
            crate::pin::count()
        ));
        let title = windows::core::HSTRING::from("SmartSnip");
        let result = windows::Win32::UI::WindowsAndMessaging::MessageBoxW(
            hwnd,
            windows::core::PCWSTR(text.as_ptr()),
            windows::core::PCWSTR(title.as_ptr()),
            windows::Win32::UI::WindowsAndMessaging::MB_YESNO
                | windows::Win32::UI::WindowsAndMessaging::MB_ICONQUESTION,
        );
        result.0 == 6 // IDYES
    }
}

/// 主窗口 wnd_proc 中转：处理托盘回调与菜单命令；返回 Some 表示已消费
pub fn handle_message(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    if msg == TRAY_CALLBACK {
        unsafe {
            // v0 回调语义：lparam 直接是鼠标消息
            match lparam.0 as u32 {
                x if x == WM_LBUTTONUP => {
                    let _ = PostMessageW(hwnd, ACTIVATE_SNIP, WPARAM(0), LPARAM(0));
                }
                x if x == WM_RBUTTONUP => show_context_menu(hwnd),
                _ => {}
            }
        }
        return Some(LRESULT(0));
    }
    if msg == WM_COMMAND {
        let id = (wparam.0 & 0xFFFF) as u16;
        match id {
            menu::SNIP_NOW => unsafe {
                let _ = PostMessageW(hwnd, ACTIVATE_SNIP, WPARAM(0), LPARAM(0));
            },
            menu::SETTINGS => crate::ui::settings::show(),
            menu::OPEN_CONFIG_DIR => {
                let dir = crate::config::ConfigStore::standard_dir();
                let _ = std::fs::create_dir_all(&dir);
                let _ = std::process::Command::new("explorer").arg(dir.as_os_str()).spawn();
            }
            menu::AUTOSTART_TOGGLE => {
                let result = if crate::autostart::is_enabled() {
                    crate::autostart::disable()
                } else {
                    crate::autostart::enable()
                };
                if let Err(err) = result {
                    crate::dbg_println!("自启切换失败: {err}");
                }
            }
            menu::EXIT => {
                if crate::pin::count() > 0 && !confirm_exit(hwnd) {
                    return Some(LRESULT(0));
                }
                unsafe {
                    shutdown();
                    let _ = PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
                }
            }
            _ => {}
        }
        return Some(LRESULT(0));
    }
    None
}
