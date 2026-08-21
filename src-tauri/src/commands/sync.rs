use crate::state::AppState;
use crate::storage::paths::AppPaths;
use crate::storage::store::Store;
use crate::sync::config::{
    delete_password, get_password, load_sync_config, save_sync_config, set_password, SyncConfig,
};
use crate::sync::engine::{reset_sync_state, SyncDecision, SyncEngine, SyncPreview, SyncSummary};
use crate::sync::webdav::{ReqwestWebDavClient, WebDavClient};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfigData {
    pub server_url: String,
    pub username: String,
    pub remote_dir: String,
    pub has_password: bool,
}

fn app_paths(state: &State<'_, AppState>) -> Result<AppPaths, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    Ok(store.paths.clone())
}

fn store_for(paths: &AppPaths) -> Store {
    Store::new(Some(paths.data_dir.clone()))
}

fn configured(paths: &AppPaths) -> Result<SyncConfig, String> {
    let store = store_for(paths);
    let config = load_sync_config(&store).normalized();
    if !config.is_configured() {
        return Err("请先填写 WebDAV 地址、用户名和远端目录".to_string());
    }
    Ok(config)
}

fn build_engine(paths: AppPaths) -> Result<SyncEngine, String> {
    let config = configured(&paths)?;
    let store = store_for(&paths);
    let password = get_password(&store, &config.username)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "未找到保存的密码，请重新输入密码并测试连接".to_string())?;
    let client = ReqwestWebDavClient::new(
        &config.server_url,
        &config.remote_dir,
        &config.username,
        &password,
    )
    .map_err(|e| e.to_string())?;
    SyncEngine::new(paths, Box::new(client)).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_sync_config(state: State<'_, AppState>) -> Result<SyncConfigData, String> {
    let paths = app_paths(&state)?;
    let store = store_for(&paths);
    let config = load_sync_config(&store).normalized();
    let has_password = get_password(&store, &config.username)
        .map(|p| p.is_some())
        .unwrap_or(false);
    Ok(SyncConfigData {
        server_url: config.server_url,
        username: config.username,
        remote_dir: config.remote_dir,
        has_password,
    })
}

#[tauri::command]
pub async fn set_sync_config(
    server_url: String,
    username: String,
    password: String,
    remote_dir: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let paths = app_paths(&state)?;
    let store = store_for(&paths);
    let previous = load_sync_config(&store);
    let config = SyncConfig {
        server_url,
        username,
        remote_dir,
    }
    .normalized();
    if !config.is_configured() {
        return Err("WebDAV 地址和用户名不能为空".to_string());
    }

    let config_changed = previous.server_url != config.server_url
        || previous.username != config.username
        || previous.remote_dir != config.remote_dir;

    save_sync_config(&store, &config).map_err(|e| format!("保存同步配置失败: {e}"))?;
    if !password.trim().is_empty() {
        set_password(&store, &config.username, &password)
            .map_err(|e| format!("保存密码失败: {e}"))?;
    }
    if config_changed {
        reset_sync_state(&paths).map_err(|e| format!("重置同步状态失败: {e}"))?;
    }
    Ok(())
}

#[tauri::command]
pub async fn clear_sync_config(state: State<'_, AppState>) -> Result<(), String> {
    let paths = app_paths(&state)?;
    let store = store_for(&paths);
    let previous = load_sync_config(&store);
    if !previous.username.trim().is_empty() {
        delete_password(&store, &previous.username)
            .map_err(|e| format!("删除密码失败: {e}"))?;
    }
    save_sync_config(&store, &SyncConfig::default())
        .map_err(|e| format!("清除同步配置失败: {e}"))?;
    reset_sync_state(&paths).map_err(|e| format!("重置同步状态失败: {e}"))?;
    Ok(())
}

#[tauri::command]
pub async fn test_sync_connection(
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let paths = app_paths(&state)?;
    let config = configured(&paths)?;
    let store = store_for(&paths);
    let password = match password.filter(|p| !p.trim().is_empty()) {
        Some(value) => value,
        None => get_password(&store, &config.username)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "未找到保存的密码，请重新输入密码".to_string())?,
    };
    let client = ReqwestWebDavClient::new(
        &config.server_url,
        &config.remote_dir,
        &config.username,
        &password,
    )
    .map_err(|e| e.to_string())?;
    client.ensure_root().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn preview_sync(state: State<'_, AppState>) -> Result<SyncPreview, String> {
    let paths = app_paths(&state)?;
    let mut engine = build_engine(paths)?;
    engine.preview().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn apply_sync(
    decisions: Vec<SyncDecision>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SyncSummary, String> {
    let paths = app_paths(&state)?;
    let mut engine = build_engine(paths)?;
    let _ = app.emit(
        "sync-progress",
        serde_json::json!({
            "stage": "开始",
            "current": 0,
            "total": 0,
        }),
    );

    let result = engine
        .apply(&decisions, |current, total, label| {
            let _ = app.emit(
                "sync-progress",
                serde_json::json!({
                    "stage": label,
                    "current": current + 1,
                    "total": total,
                }),
            );
        })
        .await;

    match result {
        Ok(summary) => {
            let _ = app.emit(
                "sync-complete",
                serde_json::json!({
                    "applied": summary.applied,
                    "message": summary.message,
                }),
            );
            Ok(summary)
        }
        Err(e) => {
            let _ = app.emit("sync-error", e.to_string());
            Err(e.to_string())
        }
    }
}
