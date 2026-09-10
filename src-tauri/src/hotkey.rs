//! 全局「隐藏 / 恢复」快捷键。
//!
//! 这里用 Win32 的 RegisterHotKey，而不是引入 tauri-plugin-global-shortcut：
//! 那个插件不在 Cargo.lock 里，加进来就意味着构建时必须联网拉取新 crate。
//! RegisterHotKey 只依赖系统的 user32，而且注册失败也只是没有快捷键 ——
//! 托盘菜单始终能恢复窗口，所以最坏情况不会把用户困住。
//!
//! 实现放在独立的工作线程上：该线程自己注册热键、自己抽 WM_HOTKEY 消息，
//! 不碰 Tauri 主事件循环。

use once_cell::sync::Lazy;
use std::sync::mpsc::Sender;
use std::sync::Mutex;
use tauri::AppHandle;

pub const MOD_ALT: u32 = 0x0001;
pub const MOD_CONTROL: u32 = 0x0002;
pub const MOD_SHIFT: u32 = 0x0004;
pub const MOD_WIN: u32 = 0x0008;
/// 按住不放时只触发一次
pub const MOD_NOREPEAT: u32 = 0x4000;

/// 解析 "Ctrl+Alt+H" / "Ctrl+Shift+F2" 这类描述，返回 (modifiers, virtual_key)。
///
/// 至少要求一个修饰键：否则注册一个裸字母会霸占全局输入。
pub fn parse_hotkey(spec: &str) -> Option<(u32, u32)> {
    let mut modifiers = 0u32;
    let mut virtual_key: Option<u32> = None;

    for raw in spec.split('+') {
        let part = raw.trim();
        if part.is_empty() {
            continue;
        }
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => modifiers |= MOD_CONTROL,
            "alt" => modifiers |= MOD_ALT,
            "shift" => modifiers |= MOD_SHIFT,
            "win" | "super" | "meta" => modifiers |= MOD_WIN,
            other => {
                let mut chars = other.chars();
                let first = chars.next()?;
                if chars.next().is_none() && first.is_ascii_alphanumeric() {
                    // 单个字母 / 数字：直接用其 ASCII 大写码作为虚拟键码
                    virtual_key = Some(first.to_ascii_uppercase() as u32);
                } else if let Some(digits) = other.strip_prefix('f') {
                    if let Ok(index) = digits.parse::<u32>() {
                        if (1..=24).contains(&index) {
                            virtual_key = Some(0x70 + index - 1); // VK_F1 .. VK_F24
                        }
                    }
                }
            }
        }
    }

    if modifiers == 0 {
        return None;
    }
    virtual_key.map(|vk| (modifiers, vk))
}

static HOTKEY_CHANNEL: Lazy<Mutex<Option<Sender<String>>>> = Lazy::new(|| Mutex::new(None));

/// 注册（或改注册）全局快捷键。
///
/// 第一次调用会启动工作线程；后续调用只把新配置发给这个线程，
/// 由它自己先注销旧热键再注册新的，避免两个线程抢同一个热键。
#[cfg(target_os = "windows")]
pub fn set_hotkey(app: AppHandle, spec: String) {
    let mut guard = match HOTKEY_CHANNEL.lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };

    if let Some(sender) = guard.as_ref() {
        let _ = sender.send(spec);
        return;
    }

    let (sender, receiver) = std::sync::mpsc::channel::<String>();
    *guard = Some(sender);
    drop(guard);

    let result = std::thread::Builder::new()
        .name("qingread-hotkey".to_string())
        .spawn(move || platform::worker(app, spec, receiver));

    if let Err(error) = result {
        eprintln!("[hotkey] 无法启动快捷键线程: {error}");
    }
}

#[cfg(not(target_os = "windows"))]
pub fn set_hotkey(app: AppHandle, spec: String) {
    // 目前只在 Windows 上提供全局快捷键；其余平台仍可用托盘菜单隐藏 / 恢复。
    let _ = (app, spec);
}

#[cfg(target_os = "windows")]
mod platform {
    use super::{parse_hotkey, MOD_NOREPEAT};
    use std::ffi::c_void;
    use std::sync::mpsc::{Receiver, RecvTimeoutError};
    use std::time::Duration;
    use tauri::{AppHandle, Emitter, Manager};

    const HOTKEY_ID: i32 = 0xA17E;
    const WM_HOTKEY: u32 = 0x0312;
    const PM_REMOVE: u32 = 0x0001;
    /// 轮询间隔：既用来取 WM_HOTKEY，也用来检查是否换了快捷键。
    const POLL_INTERVAL: Duration = Duration::from_millis(60);

    /// 与 Win32 的 MSG 结构逐字段对应（x64 下大小 48 字节）。
    /// 字段顺序和类型不能改，否则 PeekMessageW 会写坏栈。
    #[repr(C)]
    pub struct Msg {
        hwnd: *mut c_void,
        message: u32,
        w_param: usize,
        l_param: isize,
        time: u32,
        pt_x: i32,
        pt_y: i32,
    }

    #[link(name = "user32")]
    extern "system" {
        fn RegisterHotKey(hwnd: *mut c_void, id: i32, modifiers: u32, vk: u32) -> i32;
        fn UnregisterHotKey(hwnd: *mut c_void, id: i32) -> i32;
        fn PeekMessageW(
            msg: *mut Msg,
            hwnd: *mut c_void,
            filter_min: u32,
            filter_max: u32,
            remove: u32,
        ) -> i32;
    }

    fn toggle_main_window(app: &AppHandle) {
        let Some(win) = app.get_webview_window("main") else {
            return;
        };
        if win.is_visible().unwrap_or(false) {
            let _ = win.hide();
        } else {
            let _ = win.show();
            let _ = win.unminimize();
            let _ = win.set_focus();
        }
    }

    pub fn worker(app: AppHandle, initial_spec: String, receiver: Receiver<String>) {
        let mut spec = initial_spec;
        let mut registered = false;

        loop {
            if !registered {
                match parse_hotkey(&spec) {
                    Some((modifiers, virtual_key)) => unsafe {
                        let ok = RegisterHotKey(
                            std::ptr::null_mut(),
                            HOTKEY_ID,
                            modifiers | MOD_NOREPEAT,
                            virtual_key,
                        ) != 0;
                        if ok {
                            registered = true;
                            let _ = app.emit("hotkey-ready", spec.clone());
                        } else {
                            // 最常见的原因是快捷键已被别的程序占用
                            let _ = app.emit("hotkey-unavailable", spec.clone());
                        }
                    },
                    None => {
                        let _ = app.emit("hotkey-unavailable", spec.clone());
                    }
                }
            }

            let mut message: Msg = unsafe { std::mem::zeroed() };
            let has_message =
                unsafe { PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) } != 0;
            if has_message {
                if message.message == WM_HOTKEY && message.w_param == HOTKEY_ID as usize {
                    toggle_main_window(&app);
                }
                continue;
            }

            match receiver.recv_timeout(POLL_INTERVAL) {
                Ok(next) => {
                    spec = next;
                    if registered {
                        unsafe {
                            UnregisterHotKey(std::ptr::null_mut(), HOTKEY_ID);
                        }
                        registered = false;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }

        if registered {
            unsafe {
                UnregisterHotKey(std::ptr::null_mut(), HOTKEY_ID);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_recommended_default_hotkey() {
        assert_eq!(
            parse_hotkey("Ctrl+Alt+H"),
            Some((MOD_CONTROL | MOD_ALT, 0x48))
        );
    }

    #[test]
    fn parses_hotkey_case_insensitively_and_ignores_spaces() {
        assert_eq!(
            parse_hotkey(" ctrl + SHIFT + f2 "),
            Some((MOD_CONTROL | MOD_SHIFT, 0x71))
        );
    }

    #[test]
    fn parses_win_and_digit_keys() {
        assert_eq!(parse_hotkey("Win+9"), Some((MOD_WIN, 0x39)));
    }

    #[test]
    fn rejects_hotkeys_without_a_modifier() {
        assert_eq!(parse_hotkey("H"), None);
        assert_eq!(parse_hotkey(""), None);
        assert_eq!(parse_hotkey("Ctrl+"), None);
    }

    #[test]
    fn rejects_unknown_key_names() {
        assert_eq!(parse_hotkey("Ctrl+Enter"), None);
        assert_eq!(parse_hotkey("Ctrl+F99"), None);
    }

    /// MSG 的大小必须与 Win32 一致，否则 PeekMessageW 会越界写入。
    #[cfg(all(target_os = "windows", target_pointer_width = "64"))]
    #[test]
    fn win32_msg_layout_is_48_bytes_on_x64() {
        assert_eq!(std::mem::size_of::<platform::Msg>(), 48);
    }
}
