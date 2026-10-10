use tauri::{AppHandle, Emitter, Manager};

pub fn request_close(app: &AppHandle, quit: bool) {
    if let Some(window) = app.get_webview_window("main") {
        if let Err(error) = window.emit("prepare-reader-close", quit) {
            eprintln!("无法请求保存阅读位置: {error}");
        }
    }
}

#[tauri::command]
pub fn complete_reader_close(app: AppHandle, quit: bool) -> Result<(), String> {
    crate::commands::window::persist_current_layout(&app).map_err(|e| e.to_string())?;
    if quit {
        if let Some(tray) = app.tray_by_id("main-tray") {
            let _ = tray.set_visible(false);
        }
        app.exit(0);
    } else {
        crate::tray::hide_to_tray(&app).map_err(|e| e.to_string())?;
    }
    Ok(())
}
