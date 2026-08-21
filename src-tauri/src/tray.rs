use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Manager};

use crate::state::AppState;

/// 启动时根据用户设置创建系统托盘
pub fn setup_tray(app: &App) -> tauri::Result<()> {
    let behavior = get_close_behavior(app);
    if behavior == "minimize_to_tray" {
        create_tray_icon(app)?;
    }
    Ok(())
}

/// 在运行时动态更新托盘可见性，无需重启
pub fn update_tray_for_behavior(app: &AppHandle, behavior: &str) {
    let existing = app.tray_by_id("main-tray");
    match behavior {
        "minimize_to_tray" => {
            if existing.is_none() {
                // 尚未创建托盘 → 现在创建
                if let Err(e) = create_tray_icon(app) {
                    eprintln!("[tray] 创建托盘失败: {}", e);
                }
            }
            // 已存在则无需操作
        }
        _ => {
            if let Some(tray) = existing {
                // 隐藏托盘图标（Tauri 2 不支持销毁，但隐藏后不可见即等效）
                let _ = tray.set_visible(false);
            }
        }
    }
}

fn get_close_behavior(app: &impl Manager<tauri::Wry>) -> String {
    app.state::<AppState>()
        .store
        .lock()
        .ok()
        .map(|s| s.load_settings().close_behavior)
        .unwrap_or_else(|| "quit".to_string())
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
        .tooltip("EpubReader 电子书阅读器")
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
