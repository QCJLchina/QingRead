use crate::state::AppState;
use crate::storage::paths::AppPaths;
use crate::storage::store::Store;
use crate::sync::config::{
    delete_password, get_password, load_sync_config, save_sync_config, set_password, SyncConfig,
};
use crate::sync::engine::{reset_sync_state, SyncDecision, SyncEngine, SyncPreview, SyncSummary};
use crate::sync::webdav::{ReqwestWebDavClient, WebDavClient};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

/// 防止同一进程内 preview/apply 同时读写本地文件和 sync_state.json。
static SYNC_LOCK: Lazy<tokio::sync::Mutex<()>> = Lazy::new(|| tokio::sync::Mutex::new(()));

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

    let username_changed = previous.username != config.username;
    if username_changed
        && password.trim().is_empty()
        && get_password(&store, &config.username)
            .map_err(|e| format!("读取新用户名密码失败: {e}"))?
            .is_none()
    {
        return Err("更换 WebDAV 用户名时必须输入新密码".to_string());
    }

    let config_changed = previous.server_url != config.server_url
        || username_changed
        || previous.remote_dir != config.remote_dir;

    if !password.trim().is_empty() {
        set_password(&store, &config.username, &password)
            .map_err(|e| format!("保存密码失败: {e}"))?;
    }
    save_sync_config(&store, &config).map_err(|e| format!("保存同步配置失败: {e}"))?;
    if username_changed && !previous.username.trim().is_empty() {
        delete_password(&store, &previous.username)
            .map_err(|e| format!("清理旧密码失败: {e}"))?;
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
    client.ensure_sync_dirs().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn preview_sync(state: State<'_, AppState>) -> Result<SyncPreview, String> {
    let _sync_guard = SYNC_LOCK.lock().await;
    let paths = app_paths(&state)?;
    let mut engine = build_engine(paths)?;
    engine.preview().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn apply_sync(
    decisions: Vec<SyncDecision>,
    expected_local_fingerprint: Option<String>,
    expected_remote_fingerprint: Option<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SyncSummary, String> {
    let _sync_guard = SYNC_LOCK.lock().await;
    let expected_local_fingerprint = expected_local_fingerprint
        .ok_or_else(|| "同步计划缺少本地快照，请重新预览同步".to_string())?;
    let expected_remote_fingerprint = expected_remote_fingerprint
        .ok_or_else(|| "同步计划缺少远端快照，请重新预览同步".to_string())?;
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
        .apply_with_fingerprints(
            &decisions,
            &expected_local_fingerprint,
            &expected_remote_fingerprint,
            |current, total, label| {
            let _ = app.emit(
                "sync-progress",
                serde_json::json!({
                    "stage": label,
                    "current": current + 1,
                    "total": total,
                }),
            );
            },
        )
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
