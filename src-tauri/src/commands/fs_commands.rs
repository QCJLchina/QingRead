use std::path::PathBuf;
use tauri::command;

#[command]
pub async fn reveal_in_folder(path: String) -> Result<(), String> {
    let path = PathBuf::from(&path);
    if !path.exists() {
        return Err("Path does not exist".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg("/select,")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "linux")]
    {
        let dir = path.parent().unwrap_or(&path);
        std::process::Command::new("xdg-open")
            .arg(dir)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[command]
pub async fn get_app_version() -> Result<String, String> {
    Ok(env!("CARGO_PKG_VERSION").to_string())
}
