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
    close_to_tray || low_distraction
}

/// 隐藏窗口之前调用：托盘一定要在而且要可见，
/// 否则窗口藏起来后用户没有任何办法把它找回来（隐藏的窗口也不在任务栏里）。
pub fn ensure_recovery_entry(app: &AppHandle) {
    if let Err(e) = ensure_tray_icon(app) {
        eprintln!("[tray] 创建托盘失败: {}", e);
    }
    set_tray_visible(app, true);
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

    // 使用与窗口不同的图标作为托盘图标，并标记为 template
    // template 图标：Windows 会根据系统主题自动着色（黑底白图标 / 白底黑图标）
    // 避免在系统托盘显示彩色应用图标
    let tray_icon = create_tray_template_icon();

    let _tray = TrayIconBuilder::with_id("main-tray")
        .icon(tray_icon)
        .icon_as_template(true)
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
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }
            "quit" => {
                app.exit(0);
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
                        let _ = window.hide();
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

/// 创建一个 32x32 书本图标（黑白模板）
/// template 图标的规则：
/// - 黑色像素 + 透明度 = 显示为前景色（白色 in 暗色主题，黑色 in 亮色主题）
/// - 完全透明 = 不显示
fn create_tray_template_icon() -> Image<'static> {
    const SIZE: u32 = 32;
    let mut data = vec![0u8; (SIZE * SIZE * 4) as usize];

    for y in 0..SIZE {
        for x in 0..SIZE {
            let i = ((y * SIZE + x) * 4) as usize;

            // 书本外框：22-28 列，8-26 行
            let in_book = x >= 8 && x <= 23 && y >= 6 && y <= 25;
            // 书脊：7-9 列
            let is_spine = x >= 7 && x <= 9 && y >= 6 && y <= 25;
            // 页面：11-22 列，8-24 行
            let is_page = x >= 11 && x <= 22 && y >= 8 && y <= 24;
            // 边框
            let is_border = in_book && (x == 8 || x == 23 || y == 6 || y == 25);
            // 书脊线
            let is_spine_line = is_spine;
            // 页面横线（文本纹理）
            let is_page_line = is_page && !is_border && (y == 11 || y == 14 || y == 17 || y == 20);

            let is_filled = is_border || is_spine_line || is_page_line;

            if is_filled {
                data[i] = 0;
                data[i + 1] = 0;
                data[i + 2] = 0;
                data[i + 3] = 255;
            } else {
                data[i] = 0;
                data[i + 1] = 0;
                data[i + 2] = 0;
                data[i + 3] = 0;
            }
        }
    }

    Image::new_owned(data, SIZE, SIZE)
}
