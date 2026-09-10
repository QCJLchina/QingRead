use crate::state::AppState;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsData {
    pub theme: String,
    pub font_size: f32,
    pub line_height: f32,
    pub font_family: String,
    pub custom_bg_image: Option<String>,
    pub data_dir: Option<String>,
    pub close_behavior: String,
    /// 阅读模式：paged / scroll。None 表示用户从未选择过。
    #[serde(default)]
    pub reading_mode: Option<String>,
    #[serde(default)]
    pub content_width: Option<f32>,
    #[serde(default)]
    pub content_padding: Option<f32>,
}

impl From<crate::storage::store::AppSettings> for SettingsData {
    fn from(s: crate::storage::store::AppSettings) -> Self {
        Self {
            theme: s.theme,
            font_size: s.font_size,
            line_height: s.line_height,
            font_family: s.font_family,
            custom_bg_image: s.custom_bg_image,
            data_dir: s.data_dir,
            close_behavior: s.close_behavior,
            reading_mode: s.reading_mode,
            content_width: s.content_width,
            content_padding: s.content_padding,
        }
    }
}

impl Into<crate::storage::store::AppSettings> for SettingsData {
    fn into(self) -> crate::storage::store::AppSettings {
        crate::storage::store::AppSettings {
            theme: self.theme,
            font_size: self.font_size,
            line_height: self.line_height,
            font_family: self.font_family,
            custom_bg_image: self.custom_bg_image,
            data_dir: self.data_dir,
            close_behavior: self.close_behavior,
            reading_mode: self.reading_mode,
            content_width: self.content_width,
            content_padding: self.content_padding,
        }
    }
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<SettingsData, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    Ok(store.load_settings().into())
}

#[tauri::command]
pub async fn save_settings(
    settings: SettingsData,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let old_behavior = {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        let old = store.load_settings().close_behavior;
        store
            .save_settings(&settings.clone().into())
            .map_err(|e| format!("Failed to save settings: {}", e))?;
        old
    };

    // 关闭行为变更：实时更新托盘，不再需要重启
    if old_behavior != settings.close_behavior {
        crate::tray::refresh_tray(&app);
    }

    Ok(())
}

#[tauri::command]
pub async fn set_data_dir(
    path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // 空字符串 → 恢复默认（None）
    let new_data_dir: Option<std::path::PathBuf> = if path.trim().is_empty() {
        None
    } else {
        let p = std::path::PathBuf::from(&path);
        if !p.exists() {
            return Err(format!("目录不存在: {}", path));
        }
        if !p.is_dir() {
            return Err(format!("不是一个目录: {}", path));
        }
        Some(p)
    };

    let mut store = state.store.lock().map_err(|e| e.to_string())?;

    // 先确保新数据目录可创建
    let new_paths = crate::storage::paths::AppPaths::new(new_data_dir.clone());
    new_paths.ensure_dirs().map_err(|e| {
        format!("无法创建新数据目录 {}: {}", new_paths.data_dir.display(), e)
    })?;

    // 迁移旧数据：迁移元数据 + 书籍本体（NSIS 安装场景下 books/ 必须带走，
    // 否则切完目录打开书架会全部"文件找不到"）。
    let old_settings_file = store.paths.settings_file.clone();
    if old_settings_file != new_paths.settings_file {
        // settings.json
        if old_settings_file.exists() && !new_paths.settings_file.exists() {
            std::fs::copy(&old_settings_file, &new_paths.settings_file)
                .map_err(|e| format!("Failed to migrate settings.json: {}", e))?;
        }
        // library.json
        if store.paths.library_file.exists() && !new_paths.library_file.exists() {
            std::fs::copy(&store.paths.library_file, &new_paths.library_file)
                .map_err(|e| format!("Failed to migrate library.json: {}", e))?;
        }
        // 封面
        if store.paths.covers_dir.exists() {
            copy_dir_recursive(&store.paths.covers_dir, &new_paths.covers_dir)
                .map_err(|e| format!("Failed to migrate covers: {}", e))?;
        }
        // 阅读进度
        if store.paths.progress_dir.exists() {
            copy_dir_recursive(&store.paths.progress_dir, &new_paths.progress_dir)
                .map_err(|e| format!("Failed to migrate progress: {}", e))?;
        }
        // 书籍本体（导入时已经复制到 data_dir/books/）
        if store.paths.books_dir.exists() {
            copy_dir_recursive(&store.paths.books_dir, &new_paths.books_dir)
                .map_err(|e| format!("Failed to migrate books: {}", e))?;
        }
        // 自定义背景图
        if store.paths.assets_dir.exists() {
            copy_dir_recursive(&store.paths.assets_dir, &new_paths.assets_dir)
                .map_err(|e| format!("Failed to migrate assets: {}", e))?;
        }
        // 删除墓碑
        if store.paths.tombstones_dir.exists() {
            copy_dir_recursive(&store.paths.tombstones_dir, &new_paths.tombstones_dir)
                .map_err(|e| format!("Failed to migrate tombstones: {}", e))?;
        }
        // 同步配置与状态
        for (old, new) in [
            (&store.paths.sync_config_file, &new_paths.sync_config_file),
            (&store.paths.sync_state_file, &new_paths.sync_state_file),
        ] {
            if old.exists() && !new.exists() {
                std::fs::copy(old, new)
                    .map_err(|e| format!("Failed to migrate sync file: {}", e))?;
            }
        }
    }

    // 切换到新 Store
    let new_store = crate::storage::store::Store::new(new_data_dir.clone());
    *store = new_store;

    // 主动更新 settings.json 的 data_dir 字段，保证和实际数据目录一致
    // （空串恢复默认时序列化为 null，避免 settings.json 里残留 Some("")）
    let mut current = store.load_settings();
    current.data_dir = new_data_dir.as_ref().map(|p| p.to_string_lossy().to_string());
    store
        .save_settings(&current)
        .map_err(|e| format!("Failed to write settings.json: {}", e))?;

    // 关键修复：把同样的 settings.json 同步到「默认数据目录」(没有 data_dir 字段)，
    // 仅用于持久化 data_dir 选择，main.rs 启动时从这里读。
    // 这样用户改完目录 → 重启 → 启动逻辑会从这里读到新路径。
    persist_data_dir_choice(new_data_dir.as_ref());

    Ok(())
}

/// 把 data_dir 选择写到默认数据目录的 settings.json 里。
/// 这个文件只用来记录「数据目录在哪」，其他字段不动；新数据目录里仍保留完整 settings。
fn persist_data_dir_choice(new_data_dir: Option<&std::path::PathBuf>) {
    use serde_json::json;
    let default_paths = crate::storage::paths::AppPaths::new(None);
    let is_default = match new_data_dir {
        None => true,
        Some(p) => *p == default_paths.data_dir,
    };
    if is_default {
        // 当前 data_dir 就是默认目录，前面 store.save_settings 已经写好完整 settings。
        // 默认目录就是当前 data_dir，所以默认目录的 settings.json 也就是新 store 的 settings.json。
        return;
    }
    let file = default_paths.settings_file.clone();
    if let Some(parent) = file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut current: serde_json::Value = std::fs::read_to_string(&file)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| json!({}));
    current["data_dir"] = match new_data_dir {
        Some(p) => json!(p.to_string_lossy()),
        None => json!(null),
    };
    if let Ok(text) = serde_json::to_string_pretty(&current) {
        if let Err(e) = std::fs::write(&file, text) {
            eprintln!("[persist_data_dir_choice] 写默认 settings.json 失败: {}", e);
        }
    }
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_dir_recursive_merges_into_existing_destination() {
        let temp_dir = std::env::temp_dir().join(format!(
            "qingread-copy-test-{}",
            uuid::Uuid::new_v4()
        ));
        let src = temp_dir.join("src");
        let dst = temp_dir.join("dst");
        std::fs::create_dir_all(src.join("covers")).unwrap();
        std::fs::create_dir_all(dst.join("covers")).unwrap();
        std::fs::write(src.join("covers/a.jpg"), b"new-a").unwrap();
        std::fs::write(src.join("covers/b.jpg"), b"new-b").unwrap();
        std::fs::write(dst.join("covers/a.jpg"), b"old-a").unwrap();
        std::fs::write(dst.join("covers/c.jpg"), b"old-c").unwrap();

        copy_dir_recursive(&src, &dst).unwrap();

        assert_eq!(
            std::fs::read(dst.join("covers/a.jpg")).unwrap(),
            b"new-a"
        );
        assert_eq!(
            std::fs::read(dst.join("covers/b.jpg")).unwrap(),
            b"new-b"
        );
        assert_eq!(
            std::fs::read(dst.join("covers/c.jpg")).unwrap(),
            b"old-c"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}

#[tauri::command]
pub async fn restart_app(app: tauri::AppHandle) -> Result<(), String> {
    app.restart();
}
