use crate::storage::paths::AppPaths;
use crate::storage::store::atomic_write_json;
use crate::sync::engine::{Manifest, SyncState};

pub fn load(paths: &AppPaths) -> SyncState {
    std::fs::read_to_string(&paths.sync_state_file)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

pub fn save(paths: &AppPaths, state: &SyncState) -> anyhow::Result<()> {
    atomic_write_json(&paths.sync_state_file, state)
}

/// 保留设备 ID，但清空快照和待删除记录，以便切换远端目录后重新首同步。
pub fn reset(paths: &AppPaths) -> anyhow::Result<()> {
    let mut state = load(paths);
    state.last_local.clear();
    state.last_remote = Manifest::default();
    state.pending_deletes.clear();
    save(paths, &state)
}
