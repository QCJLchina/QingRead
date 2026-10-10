use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Manager};

use crate::state::AppState;

/// 启动时建立托盘图标，并按当前偏好决定是否显示。
pub fn setup_tray(app: &App) -> tauri::Result<()> {
    create_tray_icon(app)?;
    set_tray_visible(app.handle(), tray_should_be_visible(app.handle()));
    Ok(())
}

/// 关闭行为或低干扰模式变化后调用，实时更新托盘可见性，无需重启。
pub fn refresh_tray(app: &AppHandle) {
    if let Err(e) = ensure_tray_icon(app) {
        eprintln!("[tray] 创建托盘失败: {}", e);
    }
    set_tray_visible(app, tray_should_be_visible(app));
}

/// 什么时候需要托盘图标：
/// - 关闭行为是「最小化到托盘」；
/// - 或者开启了低干扰模式 —— 窗口随时可能被隐藏，必须留一个能找回它的入口，
///   这条不依赖「关闭到托盘」设置。
fn tray_should_be_visible(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();
    let Ok(store) = state.store.lock() else {
        return true;
    };
    let close_to_tray = store.load_settings().close_behavior == "minimize_to_tray";
    let low_distraction = store.load_window_settings().low_distraction;
    let hidden = app.get_webview_window("main")
        .map(|window| !window.is_visible().unwrap_or(true))
        .unwrap_or(false);
    close_to_tray || low_distraction || hidden
}

/// 隐藏窗口之前调用：托盘一定要在而且要可见，
/// 否则窗口藏起来后用户没有任何办法把它找回来（隐藏的窗口也不在任务栏里）。
pub fn hide_to_tray(app: &AppHandle) -> tauri::Result<()> {
    ensure_tray_icon(app)?;
    if let Some(tray) = app.tray_by_id("main-tray") {
        tray.set_visible(true)?;
    }
    if let Some(window) = app.get_webview_window("main") {
        window.hide()?;
    }
    Ok(())
}

fn set_tray_visible(app: &AppHandle, visible: bool) {
    if let Some(tray) = app.tray_by_id("main-tray") {
        let _ = tray.set_visible(visible);
    }
}

fn ensure_tray_icon(app: &AppHandle) -> tauri::Result<()> {
    if app.tray_by_id("main-tray").is_none() {
        create_tray_icon(app)?;
    }
    Ok(())
}

fn create_tray_icon(app: &impl Manager<tauri::Wry>) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
    let hide = MenuItem::with_id(app, "hide", "隐藏主窗口", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&show, &hide, &separator, &quit])?;

    let tray_icon = Image::from_bytes(include_bytes!("../icons/32x32.png"))?;

    let _tray = TrayIconBuilder::with_id("main-tray")
        .icon(tray_icon)
        .tooltip("轻阅 / QingRead")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
            }
            "hide" => {
                let _ = hide_to_tray(app);
            }
            "quit" => {
                crate::commands::close::request_close(app, true);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    if window.is_visible().unwrap_or(false) {
                        let _ = hide_to_tray(app);
                    } else {
                        let _ = window.show();
                        let _ = window.unminimize();
                        let _ = window.set_focus();
                    }
                }
            }
        })
        .build(app)?;

    Ok(())
}
