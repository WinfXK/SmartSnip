//! Copyright (c) 2026 Winfxk (冰月)
//! Email: admin@winfxk.cn  QQ: 2508543202
//! 创建时间: 2026年11月23日
//!
//! 设置窗口：快捷键录制、贴图参数、开机自启、标注默认色、配置目录。
//! 原生控件（STATIC/BUTTON/EDIT/COMBOBOX），保存后即时生效。

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{GetStockObject, DEFAULT_GUI_FONT};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, SetFocus, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetSystemMetrics, IsWindow, MessageBoxW,
    RegisterClassW, SendMessageW, SetForegroundWindow, SetWindowTextW, ShowWindow, HMENU,
    MB_ICONERROR, MB_ICONINFORMATION, MB_OK, SM_CXSCREEN, SM_CYSCREEN, SW_SHOW, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_CLOSE, WM_COMMAND, WM_DESTROY, WM_KEYDOWN, WM_SETFONT, WM_SYSKEYDOWN,
    WNDCLASSW,
};

use crate::config::{Config, ConfigStore};
use crate::hotkey::HotKeyCombo;

const CLASS_NAME: &str = "SmartSnip_SettingsWindow";

const ID_HOTKEY_EDIT: i32 = 1001;
const ID_HOTKEY_RECORD: i32 = 1002;
const ID_PIN_MAX: i32 = 1003;
const ID_PIN_OPACITY: i32 = 1004;
const ID_BLUR: i32 = 1005;
const ID_AUTOSTART: i32 = 1006;
const ID_COLOR: i32 = 1007;
const ID_SAVE: i32 = 1008;
const ID_CANCEL: i32 = 1009;
const ID_OPEN_DIR: i32 = 1010;

const COLOR_NAMES: [&str; 4] = ["red", "yellow", "blue", "black"];
const COLOR_LABELS: [&str; 4] = ["红", "黄", "蓝", "黑"];

struct Controls {
    hotkey_edit: HWND,
    pin_max: HWND,
    pin_opacity: HWND,
    blur: HWND,
    autostart: HWND,
    color: HWND,
}

struct SettingsState {
    hwnd: HWND,
    cfg: Config,
    ctl: Controls,
    recording: bool,
    recorded: Option<HotKeyCombo>,
    /// 录制期间已暂停全局热键
    suspended: bool,
}

static mut SETTINGS: Option<SettingsState> = None;

/// 打开设置窗口；已打开时置前
pub fn show() {
    unsafe {
        if let Some(st) = SETTINGS.as_ref() {
            if IsWindow(st.hwnd).as_bool() {
                let _ = ShowWindow(st.hwnd, SW_SHOW);
                let _ = SetForegroundWindow(st.hwnd);
                return;
            }
        }
        let store = ConfigStore::new(ConfigStore::standard_dir());
        let cfg = store.load();
        let Ok(hinstance) = GetModuleHandleW(None) else { return };
        let class_name = HSTRING::from(CLASS_NAME);
        static REGISTERED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        if REGISTERED.get().is_none() {
            let wc = WNDCLASSW {
                lpfnWndProc: Some(wnd_proc),
                hInstance: hinstance.into(),
                hIcon: crate::icon::app_icon(),
                lpszClassName: PCWSTR(class_name.as_ptr()),
                ..Default::default()
            };
            RegisterClassW(&wc);
            REGISTERED.set(()).ok();
        }
        let sw = GetSystemMetrics(SM_CXSCREEN);
        let sh = GetSystemMetrics(SM_CYSCREEN);
        let (w, h) = (440, 380);
        let hwnd = match CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(class_name.as_ptr()),
            windows::core::w!("SmartSnip 设置"),
            WINDOW_STYLE(0x12C8_0000), // WS_VISIBLE|WS_CAPTION|WS_SYSMENU|WS_CLIPCHILDREN
            (sw - w) / 2,
            (sh - h) / 2,
            w,
            h,
            None,
            None,
            HINSTANCE(hinstance.0),
            None,
        ) {
            Ok(hwnd) => hwnd,
            Err(err) => {
                crate::logging::error(&format!("设置窗口创建失败: {err}"));
                return;
            }
        };
        let ctl = create_controls(hwnd, hinstance.into());
        populate(&ctl, &cfg);
        SETTINGS = Some(SettingsState {
            hwnd,
            cfg,
            ctl,
            recording: false,
            recorded: None,
            suspended: false,
        });
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
    }
}

unsafe fn ctrl(
    parent: HWND,
    class: PCWSTR,
    text: PCWSTR,
    style: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    id: i32,
    hinst: HINSTANCE,
) -> HWND {
    CreateWindowExW(
        WINDOW_EX_STYLE(0),
        class,
        text,
        WINDOW_STYLE(style),
        x,
        y,
        w,
        h,
        parent,
        HMENU(id as isize as *mut core::ffi::c_void),
        hinst,
        None,
    )
    .unwrap_or_default()
}

unsafe fn apply_font(hwnd: HWND) {
    let font = GetStockObject(DEFAULT_GUI_FONT);
    let _ = SendMessageW(hwnd, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
}

unsafe fn mk_label(hwnd: HWND, hinst: HINSTANCE, text: &str, y: i32, w: i32) -> HWND {
    let class = HSTRING::from("STATIC");
    let t = HSTRING::from(text);
    ctrl(
        hwnd,
        PCWSTR(class.as_ptr()),
        PCWSTR(t.as_ptr()),
        0x5000_0000, // WS_CHILD|WS_VISIBLE
        16,
        y,
        w,
        20,
        0,
        hinst,
    )
}

unsafe fn create_controls(hwnd: HWND, hinst: HINSTANCE) -> Controls {
    let class = |s: &'static str| HSTRING::from(s);
    let static_c = class("STATIC");
    let edit_c = class("EDIT");
    let button_c = class("BUTTON");
    let combo_c = class("COMBOBOX");
    let pcw = |s: &HSTRING| PCWSTR(s.as_ptr());

    let mut all = Vec::new();
    all.push(mk_label(hwnd, hinst, "快捷键", 16, 70));
    let edit = ctrl(
        hwnd,
        pcw(&edit_c),
        PCWSTR::null(),
        0x5000_0000 | 0x0080_0000 | 0x0000_0800 | 0x0000_0080, // CHILD|VISIBLE|BORDER|READONLY|AUTOHSCROLL
        92,
        14,
        196,
        24,
        ID_HOTKEY_EDIT,
        hinst,
    );
    all.push(edit);
    let rec = ctrl(
        hwnd,
        pcw(&button_c),
        pcw(&HSTRING::from("录制")),
        0x5000_0000 | 0x0000_0001, // CHILD|VISIBLE|BS_DEFPUSHBUTTON
        296,
        14,
        100,
        24,
        ID_HOTKEY_RECORD,
        hinst,
    );
    all.push(rec);
    let hint = HSTRING::from("按下组合键，需含 Ctrl/Alt/Shift/Win");
    let h = ctrl(
        hwnd,
        pcw(&static_c),
        pcw(&hint),
        0x5000_0000,
        92,
        42,
        310,
        20,
        0,
        hinst,
    );
    all.push(h);

    all.push(mk_label(hwnd, hinst, "贴图上限", 78, 70));
    let pmax = ctrl(
        hwnd,
        pcw(&edit_c),
        PCWSTR::null(),
        0x5000_0000 | 0x0080_0000 | 0x0000_2000 | 0x0000_0080, // +ES_NUMBER
        92,
        76,
        80,
        24,
        ID_PIN_MAX,
        hinst,
    );
    all.push(pmax);

    all.push(mk_label(hwnd, hinst, "初始不透明度(%)", 112, 90));
    let pop = ctrl(
        hwnd,
        pcw(&edit_c),
        PCWSTR::null(),
        0x5000_0000 | 0x0080_0000 | 0x0000_2000 | 0x0000_0080,
        188,
        110,
        80,
        24,
        ID_PIN_OPACITY,
        hinst,
    );
    all.push(pop);

    let blur_t = HSTRING::from("失焦自动淡化");
    let blur = ctrl(
        hwnd,
        pcw(&button_c),
        pcw(&blur_t),
        0x5000_0000 | 0x0000_0003, // BS_AUTOCHECKBOX
        92,
        146,
        200,
        24,
        ID_BLUR,
        hinst,
    );
    all.push(blur);

    let auto_t = HSTRING::from("开机自启");
    let auto = ctrl(
        hwnd,
        pcw(&button_c),
        pcw(&auto_t),
        0x5000_0000 | 0x0000_0003,
        92,
        176,
        200,
        24,
        ID_AUTOSTART,
        hinst,
    );
    all.push(auto);

    all.push(mk_label(hwnd, hinst, "标注默认色", 210, 80));
    let color = ctrl(
        hwnd,
        pcw(&combo_c),
        PCWSTR::null(),
        0x5000_0000 | 0x0020_0000 | 0x0000_0003, // WS_VSCROLL|CBS_DROPDOWNLIST
        92,
        208,
        120,
        200,
        ID_COLOR,
        hinst,
    );
    all.push(color);

    let open_t = HSTRING::from("打开配置目录");
    let open = ctrl(
        hwnd,
        pcw(&button_c),
        pcw(&open_t),
        0x5000_0000 | 0x0000_0001,
        16,
        248,
        120,
        26,
        ID_OPEN_DIR,
        hinst,
    );
    all.push(open);

    let save_t = HSTRING::from("保存");
    let save = ctrl(
        hwnd,
        pcw(&button_c),
        pcw(&save_t),
        0x5000_0000 | 0x0000_0001,
        206,
        288,
        90,
        28,
        ID_SAVE,
        hinst,
    );
    all.push(save);
    let cancel_t = HSTRING::from("取消");
    let cancel = ctrl(
        hwnd,
        pcw(&button_c),
        pcw(&cancel_t),
        0x5000_0000 | 0x0000_0001,
        306,
        288,
        90,
        28,
        ID_CANCEL,
        hinst,
    );
    all.push(cancel);

    for h in all {
        apply_font(h);
    }
    Controls {
        hotkey_edit: edit,
        pin_max: pmax,
        pin_opacity: pop,
        blur,
        autostart: auto,
        color,
    }
}

unsafe fn populate(ctl: &Controls, cfg: &Config) {
    let combo = HotKeyCombo {
        modifiers: cfg.hotkey.modifiers.clone(),
        key: cfg.hotkey.key.clone(),
    };
    set_text(ctl.hotkey_edit, &combo_display(&combo));
    set_text(ctl.pin_max, &cfg.pin.max_count.to_string());
    set_text(ctl.pin_opacity, &cfg.pin.opacity.to_string());
    set_check(ctl.blur, cfg.pin.blur_on_unfocus);
    set_check(ctl.autostart, cfg.autostart);
    let color = ctl.color;
    for label in COLOR_LABELS {
        let h = HSTRING::from(label);
        let _ = SendMessageW(
            color,
            0x0143, // CB_ADDSTRING
            WPARAM(0),
            LPARAM(h.as_ptr() as isize),
        );
    }
    let idx = COLOR_NAMES
        .iter()
        .position(|c| *c == cfg.annotate.color)
        .unwrap_or(0);
    let _ = SendMessageW(color, 0x014E, WPARAM(idx), LPARAM(0)); // CB_SETCURSEL
}

fn set_text(hwnd: HWND, text: &str) {
    if hwnd.is_invalid() {
        return;
    }
    let h = HSTRING::from(text);
    unsafe {
        let _ = SetWindowTextW(hwnd, PCWSTR(h.as_ptr()));
    }
}

fn get_text(hwnd: HWND) -> String {
    let mut buf = [0u16; 64];
    let n = unsafe { windows::Win32::UI::WindowsAndMessaging::GetWindowTextW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn set_check(hwnd: HWND, checked: bool) {
    unsafe {
        let _ = SendMessageW(
            hwnd,
            0x00F9, // BM_SETCHECK
            WPARAM(if checked { 1 } else { 0 }),
            LPARAM(0),
        );
    }
}

fn is_checked(hwnd: HWND) -> bool {
    unsafe { SendMessageW(hwnd, 0x00F0, WPARAM(0), LPARAM(0)).0 == 1 }
}

fn combo_display(combo: &HotKeyCombo) -> String {
    let mut parts: Vec<String> = combo.modifiers.iter().map(|m| mod_label(m)).collect();
    parts.push(combo.key.to_uppercase());
    parts.join("+")
}

fn mod_label(m: &str) -> String {
    match m.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => "Ctrl".into(),
        "alt" => "Alt".into(),
        "shift" => "Shift".into(),
        "win" => "Win".into(),
        other => other.to_string(),
    }
}

/// VK → 配置键名（仅支持 A~Z、0~9、F1~F12）
fn vk_name(vk: u32) -> Option<String> {
    match vk {
        0x41..=0x5A => Some(((vk as u8) as char).to_ascii_lowercase().to_string()),
        0x30..=0x39 => Some(((vk as u8) as char).to_string()),
        0x70..=0x7B => Some(format!("F{}", vk - 0x6F)),
        _ => None,
    }
}

fn is_modifier_vk(vk: u32) -> bool {
    matches!(vk, 0x10 | 0x11 | 0x12 | 0x5B | 0x5C | 0xA0..=0xA5)
}

unsafe fn capture_combo(vk: u32) -> Option<HotKeyCombo> {
    let mut modifiers = Vec::new();
    if GetKeyState(VK_CONTROL.0 as i32) < 0 {
        modifiers.push("ctrl".to_string());
    }
    if GetKeyState(VK_MENU.0 as i32) < 0 {
        modifiers.push("alt".to_string());
    }
    if GetKeyState(VK_SHIFT.0 as i32) < 0 {
        modifiers.push("shift".to_string());
    }
    if GetKeyState(VK_LWIN.0 as i32) < 0 || GetKeyState(VK_RWIN.0 as i32) < 0 {
        modifiers.push("win".to_string());
    }
    let key = vk_name(vk)?;
    if modifiers.is_empty() {
        return None;
    }
    Some(HotKeyCombo { modifiers, key })
}

fn msg(hwnd: HWND, text: &str, title: &str, error: bool) {
    let text = HSTRING::from(text);
    let title = HSTRING::from(title);
    let flags = MB_OK | if error { MB_ICONERROR } else { MB_ICONINFORMATION };
    unsafe {
        MessageBoxW(
            hwnd,
            PCWSTR(text.as_ptr()),
            PCWSTR(title.as_ptr()),
            flags,
        );
    }
}

unsafe fn save(hwnd: HWND) -> bool {
    let Some(st) = SETTINGS.as_mut() else { return false };
    let mut cfg = st.cfg.clone();
    if let Some(combo) = st.recorded.clone() {
        cfg.hotkey.modifiers = combo.modifiers.clone();
        cfg.hotkey.key = combo.key.clone();
    }
    let max_count = get_text(st.ctl.pin_max).trim().parse::<u32>().unwrap_or(cfg.pin.max_count);
    let opacity = get_text(st.ctl.pin_opacity).trim().parse::<u32>().unwrap_or(cfg.pin.opacity as u32);
    cfg.pin.max_count = max_count.clamp(1, 100);
    cfg.pin.opacity = opacity.clamp(40, 100) as u8;
    cfg.pin.blur_on_unfocus = is_checked(st.ctl.blur);
    let auto = is_checked(st.ctl.autostart);
    let color_idx = SendMessageW(st.ctl.color, 0x0147, WPARAM(0), LPARAM(0)).0 as usize; // CB_GETCURSEL
    cfg.annotate.color = COLOR_NAMES[color_idx.min(COLOR_NAMES.len() - 1)].to_string();

    let new_combo = HotKeyCombo {
        modifiers: cfg.hotkey.modifiers.clone(),
        key: cfg.hotkey.key.clone(),
    };
    if new_combo != st.cfg.hotkey_combo() {
        if let Err(err) = crate::app::rebind_hotkey(&new_combo) {
            msg(hwnd, &format!("快捷键注册失败：{err}"), "设置", true);
            return false;
        }
    }

    if auto != st.cfg.autostart {
        let result = if auto {
            crate::autostart::enable()
        } else {
            crate::autostart::disable()
        };
        if let Err(err) = result {
            msg(hwnd, &format!("开机自启设置失败：{err}"), "设置", true);
            return false;
        }
    }
    cfg.autostart = auto;

    let store = ConfigStore::new(ConfigStore::standard_dir());
    store.save(&cfg);
    crate::config::set_runtime(cfg.clone());
    st.cfg = cfg;
    let _ = DestroyWindow(hwnd);
    true
}

impl Config {
    fn hotkey_combo(&self) -> HotKeyCombo {
        HotKeyCombo {
            modifiers: self.hotkey.modifiers.clone(),
            key: self.hotkey.key.clone(),
        }
    }
}

fn start_recording(hwnd: HWND) {
    unsafe {
        let Some(st) = SETTINGS.as_mut() else { return };
        st.recording = true;
        st.recorded = None;
        if !st.suspended {
            crate::app::suspend_hotkey();
            st.suspended = true;
        }
        set_text(st.ctl.hotkey_edit, "请按下组合键…");
        let _ = SetFocus(hwnd);
    }
}

/// 结束录制并恢复全局热键
fn end_recording(st: &mut SettingsState) {
    st.recording = false;
    if st.suspended {
        crate::app::resume_hotkey();
        st.suspended = false;
    }
}

extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_COMMAND => {
                let id = (wparam.0 & 0xFFFF) as i32;
                match id {
                    ID_HOTKEY_RECORD => start_recording(hwnd),
                    ID_SAVE => {
                        save(hwnd);
                    }
                    ID_CANCEL => {
                        let _ = DestroyWindow(hwnd);
                    }
                    ID_OPEN_DIR => {
                        let dir = ConfigStore::standard_dir();
                        let _ = std::fs::create_dir_all(&dir);
                        let _ = std::process::Command::new("explorer").arg(dir.as_os_str()).spawn();
                    }
                    _ => {}
                }
                LRESULT(0)
            }
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                let Some(st) = SETTINGS.as_mut() else { return LRESULT(0) };
                if !st.recording {
                    return DefWindowProcW(hwnd, msg, wparam, lparam);
                }
                let vk = wparam.0 as u32;
                if vk == 0x1B {
                    // ESC 取消录制
                    end_recording(st);
                    set_text(st.ctl.hotkey_edit, &combo_display(&st.cfg.hotkey_combo()));
                    return LRESULT(0);
                }
                if is_modifier_vk(vk) {
                    return LRESULT(0);
                }
                match capture_combo(vk) {
                    Some(combo) => {
                        end_recording(st);
                        st.recorded = Some(combo.clone());
                        set_text(st.ctl.hotkey_edit, &combo_display(&combo));
                    }
                    None => set_text(st.ctl.hotkey_edit, "无效组合，请加修饰键后重按"),
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                if let Some(st) = SETTINGS.as_mut() {
                    if st.suspended {
                        crate::app::resume_hotkey();
                    }
                }
                SETTINGS = None;
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
