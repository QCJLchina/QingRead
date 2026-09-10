//! 窗口形态与低干扰相关的命令。
//!
//! 所有窗口操作都经由 Rust，而不是前端直接调用 @tauri-apps/api/window：
//! 一是免去为这些动作额外开放一长串 window 能力（capabilities），
//! 二是位置钳制、尺寸持久化、托盘联动只写一遍，前端只表达「我要变成什么样」。
//!
//! 关于窗口边框：这里保留系统原生边框（decorations = true）。
//! 原生边框自带边缘拖动缩放、最小化、最大化、关闭和 Windows 11 贴靠布局，
//! 应用自己的精简工具栏放在窗口内部，负责阅读相关的操作。
//! 这样即使某个自定义标题栏动作出问题，窗口也不会变成无法移动或无法关闭。

use crate::state::AppState;
use crate::storage::store::WindowSettings;
use tauri::{AppHandle, Manager, PhysicalPosition, State, WebviewWindow};

/// 客户区下限，与 tauri.conf.json 里的 minWidth / minHeight 一致。
pub const MIN_WINDOW_WIDTH: f64 = 320.0;
pub const MIN_WINDOW_HEIGHT: f64 = 200.0;
const MAX_WINDOW_EDGE: f64 = 10000.0;

/// 钳制窗口位置时至少留在显示器内的像素，
/// 保证窄栏 / 横条被拖到屏幕边缘后仍然抓得回来。
const KEEP_VISIBLE_X: i32 = 96;
const KEEP_VISIBLE_Y: i32 = 40;

fn main_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    app.get_webview_window("main")
        .ok_or_else(|| "主窗口不存在".to_string())
}

/// 把窗口拉回当前显示器可见范围。多屏拔插、分辨率变化后调用。
pub fn ensure_on_screen(win: &WebviewWindow) {
    let Ok(position) = win.outer_position() else {
        return;
    };
    let Ok(Some(monitor)) = win.current_monitor() else {
        return;
    };

    let monitor_size = monitor.size();
    let monitor_pos = monitor.position();
    let win_w = win.outer_size().map(|s| s.width as i32).unwrap_or(0);

    let max_x = monitor_pos.x + monitor_size.width as i32 - KEEP_VISIBLE_X;
    let min_x = (monitor_pos.x - win_w + KEEP_VISIBLE_X).min(max_x);
    let max_y = monitor_pos.y + monitor_size.height as i32 - KEEP_VISIBLE_Y;
    let min_y = monitor_pos.y.min(max_y);

    let x = position.x.clamp(min_x, max_x);
    let y = position.y.clamp(min_y, max_y);
    if x != position.x || y != position.y {
        let _ = win.set_position(PhysicalPosition::new(x, y));
    }
}

/// 启动时套用上次保存的窗口形态，并返回该设置供 setup 继续使用。
///
/// 要在窗口显示之前调用，避免先闪一个默认尺寸再跳到窄栏。
pub fn apply_saved_layout_on_startup(app: &AppHandle) -> WindowSettings {
    let settings = match app.state::<AppState>().store.lock() {
        Ok(store) => store.load_window_settings(),
        Err(_) => WindowSettings::default(),
    };

    if let Some(win) = app.get_webview_window("main") {
        let width = settings.width.clamp(MIN_WINDOW_WIDTH, MAX_WINDOW_EDGE);
        let height = settings.height.clamp(MIN_WINDOW_HEIGHT, MAX_WINDOW_EDGE);
        let _ = win.set_size(tauri::LogicalSize::new(width, height));

        if let (Some(x), Some(y)) = (settings.x, settings.y) {
            let scale = win.scale_factor().unwrap_or(1.0);
            let _ = win.set_position(PhysicalPosition::new(
                (x * scale).round() as i32,
                (y * scale).round() as i32,
            ));
        }
        ensure_on_screen(&win);
        let _ = win.set_always_on_top(settings.always_on_top);
    }

    settings
}

#[tauri::command]
pub async fn get_window_settings(state: State<'_, AppState>) -> Result<WindowSettings, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    Ok(store.load_window_settings())
}

#[tauri::command]
pub async fn save_window_settings(
    settings: WindowSettings,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let previous = {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        let previous = store.load_window_settings();
        store
            .save_window_settings(&settings)
            .map_err(|e| format!("保存窗口设置失败: {e}"))?;
        previous
    };

    // 只对真正变化的项做副作用，避免每次保存都重复注册快捷键或抖动托盘。
    if previous.always_on_top != settings.always_on_top {
        if let Ok(win) = main_window(&app) {
            let _ = win.set_always_on_top(settings.always_on_top);
        }
    }
    if previous.hide_hotkey != settings.hide_hotkey {
        crate::hotkey::set_hotkey(app.clone(), settings.hide_hotkey.clone());
    }
    if previous.low_distraction != settings.low_distraction {
        crate::tray::refresh_tray(&app);
    }
    Ok(())
}

#[tauri::command]
pub async fn window_hide(app: AppHandle) -> Result<(), String> {
    // 先把托盘备好，再隐藏：这是唯一的找回途径
    crate::tray::ensure_recovery_entry(&app);
    main_window(&app)?.hide().map_err(|e| e.to_string())
}

/// 应用一个窗口形态：预设，或用户在面板里输入的宽高。
/// 记录 layout / width / height / lock_ratio，并把窗口拉回可见范围。
#[tauri::command]
pub async fn window_apply_layout(
    width: f64,
    height: f64,
    layout: String,
    lock_ratio: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let width = width.clamp(MIN_WINDOW_WIDTH, MAX_WINDOW_EDGE);
    let height = height.clamp(MIN_WINDOW_HEIGHT, MAX_WINDOW_EDGE);

    let win = main_window(&app)?;
    win.set_size(tauri::LogicalSize::new(width, height))
        .map_err(|e| e.to_string())?;

    {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        let mut settings = store.load_window_settings();
        settings.layout = layout;
        settings.width = width;
        settings.height = height;
        settings.lock_ratio = lock_ratio;
        store
            .save_window_settings(&settings)
            .map_err(|e| format!("保存窗口设置失败: {e}"))?;
    }

    ensure_on_screen(&win);
    Ok(())
}

/// 记录当前真实的窗口尺寸与位置。
///
/// 命令与窗口事件（拖动边框结束、移动窗口、退出前）共用同一份实现，
/// 保证「看到的窗口形状」和「下次启动恢复的形状」永远一致。
pub fn persist_current_layout(app: &AppHandle) -> Result<(), String> {
    let win = main_window(app)?;
    if win.is_maximized().unwrap_or(false) || win.is_fullscreen().unwrap_or(false) {
        // 最大化 / 全屏时的尺寸不是用户想要的窗口形状，不覆盖已保存的预设。
        return Ok(());
    }

    let scale = win.scale_factor().unwrap_or(1.0);
    let size = win.outer_size().map_err(|e| e.to_string())?;
    let position = win.outer_position().ok();

    let state = app.state::<AppState>();
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let mut settings = store.load_window_settings();
    settings.width = (size.width as f64 / scale).round();
    settings.height = (size.height as f64 / scale).round();
    if let Some(position) = position {
        settings.x = Some((position.x as f64 / scale).round());
        settings.y = Some((position.y as f64 / scale).round());
    }
    store
        .save_window_settings(&settings)
        .map_err(|e| format!("保存窗口设置失败: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_settings_defaults_to_standard_preset() {
        let defaults = WindowSettings::default();
        assert_eq!(defaults.layout, "standard");
        assert_eq!(defaults.width, 940.0);
        assert_eq!(defaults.height, 610.0);
        assert_eq!(defaults.hide_hotkey, "Ctrl+Alt+H");
        assert!(!defaults.always_on_top);
        assert!(!defaults.low_distraction);
        assert!(!defaults.lock_ratio);
    }

    #[test]
    fn window_settings_tolerate_partial_json() {
        let parsed: WindowSettings =
            serde_json::from_str(r#"{"layout":"slim","width":360.0,"height":610.0}"#).unwrap();
        assert_eq!(parsed.layout, "slim");
        assert_eq!(parsed.width, 360.0);
        // 未出现的字段回落到默认值，保证旧文件可读
        assert_eq!(parsed.hide_hotkey, "Ctrl+Alt+H");
        assert!(!parsed.low_distraction);
    }

    #[test]
    fn window_settings_round_trip_through_json() {
        let mut settings = WindowSettings::default();
        settings.layout = "strip".to_string();
        settings.low_distraction = true;
        settings.x = Some(-120.0);
        let text = serde_json::to_string(&settings).unwrap();
        let parsed: WindowSettings = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed, settings);
    }
}
