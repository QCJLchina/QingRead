use crate::epub::parser::EpubParser;
use crate::epub::txt_parser::TxtParser;
use crate::storage::paths::AppPaths;
use crate::storage::store::{AppSettings, Store};
use crate::sync::webdav::WebDavClient;
use anyhow::{anyhow, bail, Context};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub use crate::sync::state::reset as reset_sync_state;

const MANIFEST_FILE: &str = "manifest.json";
const VALID_IMAGE_EXTS: [&str; 5] = ["jpg", "jpeg", "png", "webp", "gif"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileMeta {
    pub sha256: String,
    pub size: u64,
    pub updated_at: u64,
    pub device_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Manifest {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub updated_at: u64,
    #[serde(default)]
    pub items: HashMap<String, FileMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SyncState {
    #[serde(default)]
    pub device_id: String,
    #[serde(default)]
    pub last_local: HashMap<String, FileMeta>,
    #[serde(default)]
    pub last_remote: Manifest,
    #[serde(default)]
    pub pending_deletes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Snapshot {
    #[serde(default)]
    pub items: HashMap<String, FileMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ActionKind {
    Upload,
    Download,
    Delete,
    Conflict,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncAction {
    pub id: String,
    pub kind: ActionKind,
    pub label: String,
    pub size: u64,
    pub updated_at: u64,
    pub local_exists: bool,
    pub remote_exists: bool,
    pub conflict_type: Option<String>,
    pub direction: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPreview {
    pub actions: Vec<SyncAction>,
    pub has_remote: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncDecision {
    pub id: String,
    pub choice: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncSummary {
    pub applied: usize,
    pub message: String,
}

pub struct SyncEngine {
    store: Store,
    client: Box<dyn WebDavClient>,
    state: SyncState,
    current_remote: Manifest,
}

impl SyncEngine {
    pub fn new(paths: AppPaths, client: Box<dyn WebDavClient>) -> anyhow::Result<Self> {
        let store = Store::new(Some(paths.data_dir));
        let mut state = crate::sync::state::load(&store.paths);
        if state.device_id.trim().is_empty() {
            state.device_id = uuid::Uuid::new_v4().to_string();
            crate::sync::state::save(&store.paths, &state)?;
        }
        Ok(Self {
            store,
            client,
            state,
            current_remote: Manifest::default(),
        })
    }

    pub async fn preview(&mut self) -> anyhow::Result<SyncPreview> {
        self.prepare_local_files()?;
        let local = self.build_local_snapshot()?;
        let remote = self.fetch_remote_manifest().await?;
        self.current_remote = remote.clone();
        let actions = self.plan_actions(&local, &remote);
        Ok(SyncPreview {
            actions,
            has_remote: !remote.items.is_empty(),
        })
    }

    pub async fn apply<F>(
        &mut self,
        decisions: &[SyncDecision],
        mut on_progress: F,
    ) -> anyhow::Result<SyncSummary>
    where
        F: FnMut(usize, usize, &str),
    {
        self.prepare_local_files()?;
        let local = self.build_local_snapshot()?;
        let remote = self.fetch_remote_manifest().await?;
        self.current_remote = remote.clone();
        let actions = self.plan_actions(&local, &remote);
        if actions.is_empty() {
            return Ok(SyncSummary {
                applied: 0,
                message: "No changes to sync".to_string(),
            });
        }

        let decision_map: HashMap<String, String> = decisions
            .iter()
            .map(|d| (d.id.clone(), d.choice.clone()))
            .collect();
        for action in &actions {
            if action.kind == ActionKind::Conflict {
                let choice = decision_map
                    .get(&action.id)
                    .ok_or_else(|| anyhow!("Unresolved conflict: {}", action.id))?;
                if choice != "local" && choice != "remote" {
                    bail!("Invalid conflict choice for {}: {}", action.id, choice);
                }
            }
        }

        let total = actions.len();
        for (index, action) in actions.iter().enumerate() {
            on_progress(index, total, &action.label);
            match action.kind {
                ActionKind::Upload => self.apply_upload(action).await?,
                ActionKind::Download => self.apply_download(action).await?,
                ActionKind::Delete => self.apply_delete(action).await?,
                ActionKind::Conflict => {
                    let choice = decision_map
                        .get(&action.id)
                        .cloned()
                        .unwrap_or_else(|| "local".to_string());
                    self.apply_conflict(action, &choice).await?;
                }
            }
        }

        self.finalize_local_state()?;
        self.upload_final_manifest().await?;

        Ok(SyncSummary {
            applied: total,
            message: format!("Applied {} sync action(s)", total),
        })
    }

    fn prepare_local_files(&mut self) -> anyhow::Result<()> {
        let mut library = self.store.load_library_checked()?;
        let mut library_changed = false;
        for entry in library.iter_mut() {
            let format = if entry.format.is_empty() {
                "epub"
            } else {
                &entry.format
            };
            let managed = self.store.paths.book_path(&entry.id, format);
            if managed.exists() {
                continue;
            }
            let source = PathBuf::from(&entry.file_path);
            if source.exists() && source.is_file() {
                atomic_copy(&source, &managed)?;
                let size = std::fs::metadata(&managed).map(|m| m.len()).unwrap_or(0);
                entry.file_path = managed.to_string_lossy().to_string();
                entry.file_size = size;
                library_changed = true;
            }
        }
        if library_changed {
            self.store.save_library(&library)?;
        }

        let mut settings = self.store.load_settings();
        if let Some(ref path_text) = settings.custom_bg_image {
            let source = PathBuf::from(path_text);
            if source.exists() && source.is_file() {
                let mut ext = source
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("png")
                    .to_lowercase();
                if !VALID_IMAGE_EXTS.contains(&ext.as_str()) {
                    ext = "png".to_string();
                }
                let dest = self.store.paths.custom_bg_path(&ext);
                if dest != source {
                    atomic_copy(&source, &dest)?;
                    clean_managed_backgrounds(&self.store.paths, Some(&dest))?;
                    settings.custom_bg_image = Some(dest.to_string_lossy().to_string());
                    self.store.save_settings(&settings)?;
                } else if source.starts_with(&self.store.paths.assets_dir) {
                    clean_managed_backgrounds(&self.store.paths, Some(&source))?;
                }
            }
        }
        Ok(())
    }

    fn build_local_snapshot(&self) -> anyhow::Result<Snapshot> {
        let mut items = HashMap::new();

        if self.store.paths.library_file.exists() {
            items.insert(
                "library.json".to_string(),
                self.file_meta_for_path("library.json", &self.store.paths.library_file)?,
            );
        }
        if self.store.paths.settings_file.exists() {
            items.insert(
                "settings.json".to_string(),
                self.file_meta_for_syncable_settings()?,
            );
        }

        let settings = self.store.load_settings();
        if let Some(ref bg_path) = settings.custom_bg_image {
            let bg = PathBuf::from(bg_path);
            if bg.exists() && bg.is_file() {
                if let Some(ext) = bg.extension().and_then(|e| e.to_str()) {
                    let key = format!("custom_bg.{}", ext.to_lowercase());
                    if validate_remote_key(&key) {
                        items.insert(key.clone(), self.file_meta_for_path(&key, &bg)?);
                    }
                }
            }
        }

        add_dir_items(
            &self.store.paths.books_dir,
            &mut items,
            |_name, id, ext| {
                if ext == "epub" || ext == "txt" {
                    Some(format!("books/{}.{}", id, ext))
                } else {
                    None
                }
            },
            &self.state.last_local,
        )?;
        add_dir_items(
            &self.store.paths.covers_dir,
            &mut items,
            |_name, id, ext| {
                if ext == "jpg" {
                    Some(format!("covers/{}.jpg", id))
                } else {
                    None
                }
            },
            &self.state.last_local,
        )?;
        add_dir_items(
            &self.store.paths.progress_dir,
            &mut items,
            |name, id, ext| {
                if ext == "json" {
                    let _ = name;
                    Some(format!("progress/{}.json", id))
                } else {
                    None
                }
            },
            &self.state.last_local,
        )?;
        add_dir_items(
            &self.store.paths.tombstones_dir,
            &mut items,
            |name, id, ext| {
                if ext == "json" {
                    let _ = name;
                    Some(format!("tombstones/{}.json", id))
                } else {
                    None
                }
            },
            &self.state.last_local,
        )?;

        Ok(Snapshot { items })
    }

    fn file_meta_for_path(&self, key: &str, path: &Path) -> anyhow::Result<FileMeta> {
        let metadata = std::fs::metadata(path).context("failed to stat local sync item")?;
        let size = metadata.len();
        let mtime = modified_ms(&metadata);
        if let Some(prev) = self.state.last_local.get(key) {
            if prev.size == size && prev.updated_at == mtime {
                return Ok(prev.clone());
            }
        }
        let sha256 = hash_file(path)?;
        let updated_at = if mtime == 0 { now_secs() } else { mtime };
        Ok(FileMeta {
            sha256,
            size,
            updated_at,
            device_id: self.state.device_id.clone(),
        })
    }

    fn file_meta_for_syncable_settings(&self) -> anyhow::Result<FileMeta> {
        let bytes = syncable_settings_bytes(&self.store.load_settings())?;
        let sha256 = hash_bytes(&bytes);
        let mtime = std::fs::metadata(&self.store.paths.settings_file)
            .map(|m| modified_ms(&m))
            .unwrap_or(0);
        if let Some(prev) = self.state.last_local.get("settings.json") {
            if prev.sha256 == sha256 {
                return Ok(prev.clone());
            }
        }
        let updated_at = if mtime == 0 { now_secs() } else { mtime };
        Ok(FileMeta {
            sha256,
            size: bytes.len() as u64,
            updated_at,
            device_id: self.state.device_id.clone(),
        })
    }

    async fn fetch_remote_manifest(&self) -> anyhow::Result<Manifest> {
        match self.client.get(MANIFEST_FILE).await? {
            Some(bytes) => serde_json::from_slice(&bytes).context("invalid remote manifest"),
            None => Ok(Manifest::default()),
        }
    }

    fn plan_actions(&self, local: &Snapshot, remote: &Manifest) -> Vec<SyncAction> {
        let mut actions = Vec::new();
        let mut handled = HashSet::new();
        let mut book_ids = HashSet::new();

        for key in local.items.keys().chain(remote.items.keys()) {
            if let Some(id) = book_id_from_key(key) {
                book_ids.insert(id.to_string());
            }
        }
        for id in book_ids {
            let local_tombstone = local
                .items
                .contains_key(&format!("tombstones/{}.json", id));
            let remote_tombstone = remote
                .items
                .contains_key(&format!("tombstones/{}.json", id));
            if local_tombstone || remote_tombstone {
                actions.extend(self.tombstoned_book_actions(local, remote, &id, &mut handled));
            }
        }
        let has_delete_vs_modify = actions
            .iter()
            .any(|a| a.conflict_type.as_deref() == Some("delete_vs_modify"));

        let mut keys: Vec<String> = local
            .items
            .keys()
            .chain(remote.items.keys())
            .cloned()
            .collect();
        keys.sort();
        keys.dedup();

        for key in keys {
            if handled.contains(&key) || !validate_remote_key(&key) {
                continue;
            }
            let local_meta = local.items.get(&key);
            let remote_meta = remote.items.get(&key);
            let local_changed = self.local_changed(&key, local);
            let remote_changed = self.remote_changed(&key, remote);

            if has_delete_vs_modify && key == "library.json" {
                if local_meta.map(|m| &m.sha256) != remote_meta.map(|m| &m.sha256) {
                    actions.push(self.action_for(
                        &key,
                        ActionKind::Conflict,
                        "delete vs modify conflict",
                        local_meta.map(|m| m.size).unwrap_or(0)
                            .max(remote_meta.map(|m| m.size).unwrap_or(0)),
                        local_meta.map(|m| m.updated_at).unwrap_or(0)
                            .max(remote_meta.map(|m| m.updated_at).unwrap_or(0)),
                        local_meta.is_some(),
                        remote_meta.is_some(),
                        Some("delete_vs_modify"),
                        "local",
                    ));
                }
                continue;
            }

            if key.starts_with("tombstones/") {
                match (local_meta, remote_meta) {
                    (Some(local_item), Some(remote_item))
                        if local_item.sha256 == remote_item.sha256 => {}
                    (Some(local_item), Some(remote_item)) => {
                        let local_ts = tombstone_timestamp(
                            &self.store.paths.tombstones_dir,
                            &key,
                        )
                        .unwrap_or(local_item.updated_at);
                        if local_ts > remote_item.updated_at {
                            actions.push(self.action_for(
                                &key,
                                ActionKind::Upload,
                                "upload tombstone",
                                local_item.size,
                                local_item.updated_at,
                                true,
                                true,
                                None,
                                "local",
                            ));
                        } else {
                            actions.push(self.action_for(
                                &key,
                                ActionKind::Download,
                                "download tombstone",
                                remote_item.size,
                                remote_item.updated_at,
                                true,
                                true,
                                None,
                                "local",
                            ));
                        }
                    }
                    (Some(local_item), None) => {
                        actions.push(self.action_for(
                            &key,
                            ActionKind::Upload,
                            "upload tombstone",
                            local_item.size,
                            local_item.updated_at,
                            true,
                            false,
                            None,
                            "remote",
                        ));
                    }
                    (None, Some(remote_item)) => {
                        actions.push(self.action_for(
                            &key,
                            ActionKind::Download,
                            "download tombstone",
                            remote_item.size,
                            remote_item.updated_at,
                            false,
                            true,
                            None,
                            "local",
                        ));
                    }
                    (None, None) => {}
                }
                continue;
            }

            match (local_meta, remote_meta) {
                (Some(local_item), Some(remote_item)) => {
                    if local_item.sha256 == remote_item.sha256 {
                        continue;
                    }
                    if local_changed && remote_changed {
                        actions.push(self.action_for(
                            &key,
                            ActionKind::Conflict,
                            "content conflict",
                            local_item.size,
                            local_item.updated_at.max(remote_item.updated_at),
                            true,
                            true,
                            Some("content"),
                            "local",
                        ));
                    } else if local_changed {
                        actions.push(self.action_for(
                            &key,
                            ActionKind::Upload,
                            "upload",
                            local_item.size,
                            local_item.updated_at,
                            true,
                            true,
                            None,
                            "remote",
                        ));
                    } else if remote_changed {
                        actions.push(self.action_for(
                            &key,
                            ActionKind::Download,
                            "download",
                            remote_item.size,
                            remote_item.updated_at,
                            true,
                            true,
                            None,
                            "local",
                        ));
                    } else {
                        actions.push(self.action_for(
                            &key,
                            ActionKind::Upload,
                            "upload",
                            local_item.size,
                            local_item.updated_at,
                            true,
                            true,
                            None,
                            "remote",
                        ));
                    }
                }
                (Some(local_item), None) => {
                    if self.state.pending_deletes.contains(&key) {
                        continue;
                    }
                    if local_changed || !self.state.last_local.contains_key(&key) {
                        actions.push(self.action_for(
                            &key,
                            ActionKind::Upload,
                            "upload",
                            local_item.size,
                            local_item.updated_at,
                            true,
                            false,
                            None,
                            "remote",
                        ));
                    } else {
                        actions.push(self.action_for(
                            &key,
                            ActionKind::Upload,
                            "restore upload",
                            local_item.size,
                            local_item.updated_at,
                            true,
                            false,
                            None,
                            "remote",
                        ));
                    }
                }
                (None, Some(remote_item)) => {
                    if self.state.pending_deletes.contains(&key) {
                        actions.push(self.action_for(
                            &key,
                            ActionKind::Delete,
                            "delete remote",
                            remote_item.size,
                            remote_item.updated_at,
                            false,
                            true,
                            None,
                            "remote",
                        ));
                    } else if remote_changed
                        || (!self.state.last_local.contains_key(&key)
                            && !self.state.last_remote.items.contains_key(&key))
                    {
                        actions.push(self.action_for(
                            &key,
                            ActionKind::Download,
                            "download",
                            remote_item.size,
                            remote_item.updated_at,
                            false,
                            true,
                            None,
                            "local",
                        ));
                    } else {
                        actions.push(self.action_for(
                            &key,
                            ActionKind::Download,
                            "download",
                            remote_item.size,
                            remote_item.updated_at,
                            false,
                            true,
                            None,
                            "local",
                        ));
                    }
                }
                (None, None) => {}
            }
        }

        actions
    }

    fn tombstoned_book_actions(
        &self,
        local: &Snapshot,
        remote: &Manifest,
        id: &str,
        handled: &mut HashSet<String>,
    ) -> Vec<SyncAction> {
        let keys = [
            format!("books/{}.epub", id),
            format!("books/{}.txt", id),
            format!("covers/{}.jpg", id),
            format!("progress/{}.json", id),
        ];
        let tombstone_key = format!("tombstones/{}.json", id);
        let has_conflict = keys.iter().any(|key| {
            let local_changed = self.local_changed(key, local);
            let remote_changed = self.remote_changed(key, remote);
            (local.items.contains_key(key) && local_changed)
                || (remote.items.contains_key(key) && remote_changed)
        });

        if has_conflict {
            for key in &keys {
                handled.insert(key.clone());
            }
            handled.insert(tombstone_key);
            let conflict_key = keys
                .iter()
                .find(|key| {
                    local.items.contains_key(*key) || remote.items.contains_key(*key)
                })
                .cloned()
                .unwrap_or_else(|| format!("books/{}.epub", id));
            let local_item = local.items.get(&conflict_key);
            let remote_item = remote.items.get(&conflict_key);
            return vec![self.action_for(
                &conflict_key,
                ActionKind::Conflict,
                "delete vs modify conflict",
                local_item
                    .map(|m| m.size)
                    .unwrap_or(0)
                    .max(remote_item.map(|m| m.size).unwrap_or(0)),
                local_item
                    .map(|m| m.updated_at)
                    .unwrap_or(0)
                    .max(remote_item.map(|m| m.updated_at).unwrap_or(0)),
                local_item.is_some(),
                remote_item.is_some(),
                Some("delete_vs_modify"),
                "local",
            )];
        }

        let mut actions = Vec::new();
        let local_tombstone = local.items.contains_key(&tombstone_key);
        let remote_tombstone = remote.items.contains_key(&tombstone_key);

        for key in keys {
            handled.insert(key.clone());
            let local_meta = local.items.get(&key);
            let remote_meta = remote.items.get(&key);

            match (local_meta, remote_meta) {
                (Some(local_item), Some(remote_item)) => {
                    if local_tombstone {
                        actions.push(self.action_for(
                            &key,
                            ActionKind::Delete,
                            "delete local",
                            local_item.size,
                            local_item.updated_at,
                            true,
                            true,
                            None,
                            "local",
                        ));
                    }
                    if remote_tombstone {
                        actions.push(self.action_for(
                            &key,
                            ActionKind::Delete,
                            "delete remote",
                            remote_item.size,
                            remote_item.updated_at,
                            true,
                            true,
                            None,
                            "remote",
                        ));
                    }
                }
                (Some(local_item), None) => {
                    actions.push(self.action_for(
                        &key,
                        ActionKind::Delete,
                        "delete local",
                        local_item.size,
                        local_item.updated_at,
                        true,
                        false,
                        None,
                        "local",
                    ));
                }
                (None, Some(remote_item)) => {
                    actions.push(self.action_for(
                        &key,
                        ActionKind::Delete,
                        "delete remote",
                        remote_item.size,
                        remote_item.updated_at,
                        false,
                        true,
                        None,
                        "remote",
                    ));
                }
                (None, None) => {}
            }
        }
        actions
    }

    fn action_for(
        &self,
        id: &str,
        kind: ActionKind,
        label: &str,
        size: u64,
        updated_at: u64,
        local_exists: bool,
        remote_exists: bool,
        conflict_type: Option<&str>,
        direction: &str,
    ) -> SyncAction {
        SyncAction {
            id: id.to_string(),
            kind,
            label: format!("{} {}", label, id),
            size,
            updated_at,
            local_exists,
            remote_exists,
            conflict_type: conflict_type.map(|s| s.to_string()),
            direction: direction.to_string(),
        }
    }

    fn local_changed(&self, key: &str, local: &Snapshot) -> bool {
        changed(local.items.get(key), self.state.last_local.get(key))
    }

    fn remote_changed(&self, key: &str, remote: &Manifest) -> bool {
        changed(
            remote.items.get(key),
            self.state.last_remote.items.get(key),
        )
    }

    async fn apply_upload(&mut self, action: &SyncAction) -> anyhow::Result<()> {
        let bytes = self.bytes_for_key(&action.id)?;
        self.client.put(&action.id, bytes.clone()).await?;
        let meta = self.meta_for_key(&action.id, &bytes)?;
        self.state.last_local.insert(action.id.clone(), meta);
        self.save_state()?;
        Ok(())
    }

    async fn apply_download(&mut self, action: &SyncAction) -> anyhow::Result<()> {
        let bytes = self
            .client
            .get(&action.id)
            .await?
            .ok_or_else(|| anyhow!("Remote item disappeared: {}", action.id))?;
        self.write_local_file(&action.id, &bytes)?;
        let meta = self.meta_for_key(&action.id, &bytes)?;
        self.state.last_local.insert(action.id.clone(), meta);
        self.save_state()?;
        Ok(())
    }

    async fn apply_delete(&mut self, action: &SyncAction) -> anyhow::Result<()> {
        if action.direction == "remote" {
            self.client.delete(&action.id).await?;
            if !self.state.pending_deletes.contains(&action.id) {
                self.state.pending_deletes.push(action.id.clone());
            }
        } else {
            self.remove_local_file(&action.id)?;
            self.state.last_local.remove(&action.id);
            if self.current_remote.items.contains_key(&action.id)
                && !self.state.pending_deletes.contains(&action.id)
            {
                self.state.pending_deletes.push(action.id.clone());
            }
        }
        self.save_state()?;
        Ok(())
    }

    async fn apply_conflict(&mut self, action: &SyncAction, choice: &str) -> anyhow::Result<()> {
        if action.conflict_type.as_deref() == Some("delete_vs_modify") {
            if let Some(id) = book_id_from_key(&action.id) {
                return self.apply_delete_vs_modify_conflict(id, choice).await;
            }
        }

        if choice == "local" {
            let bytes = self.bytes_for_key(&action.id)?;
            self.client.put(&action.id, bytes.clone()).await?;
            let meta = self.meta_for_key(&action.id, &bytes)?;
            self.state.last_local.insert(action.id.clone(), meta);
            self.save_state()?;
        } else {
            let bytes = self
                .client
                .get(&action.id)
                .await?
                .ok_or_else(|| anyhow!("Remote item disappeared: {}", action.id))?;
            self.write_local_file(&action.id, &bytes)?;
            let meta = self.meta_for_key(&action.id, &bytes)?;
            self.state.last_local.insert(action.id.clone(), meta);
            self.save_state()?;
        }
        Ok(())
    }

    async fn apply_delete_vs_modify_conflict(
        &mut self,
        id: &str,
        choice: &str,
    ) -> anyhow::Result<()> {
        let keys = [
            format!("books/{}.epub", id),
            format!("books/{}.txt", id),
            format!("covers/{}.jpg", id),
            format!("progress/{}.json", id),
        ];
        let tombstone_key = format!("tombstones/{}.json", id);
        let local_tombstone_path = self.store.paths.tombstone_path(id);
        let local_tombstone = local_tombstone_path.exists();
        let remote_tombstone = self.current_remote.items.contains_key(&tombstone_key);

        if choice == "local" {
            for key in &keys {
                if self.local_path_for_key(key)?.exists() {
                    let bytes = self.bytes_for_key(key)?;
                    self.client.put(key, bytes.clone()).await?;
                    let meta = self.meta_for_key(key, &bytes)?;
                    self.state.last_local.insert(key.clone(), meta.clone());
                    self.current_remote.items.insert(key.clone(), meta);
                } else if self.current_remote.items.contains_key(key) {
                    self.client.delete(key).await?;
                    if !self.state.pending_deletes.contains(key) {
                        self.state.pending_deletes.push(key.clone());
                    }
                    self.current_remote.items.remove(key);
                    self.state.last_local.remove(key);
                }
            }
            if local_tombstone {
                let bytes = std::fs::read(&local_tombstone_path)?;
                self.client.put(&tombstone_key, bytes.clone()).await?;
                let meta = self.meta_for_key(&tombstone_key, &bytes)?;
                self.state
                    .last_local
                    .insert(tombstone_key.clone(), meta.clone());
                self.current_remote
                    .items
                    .insert(tombstone_key.clone(), meta);
            } else {
                if remote_tombstone {
                    self.client.delete(&tombstone_key).await?;
                    if !self.state.pending_deletes.contains(&tombstone_key) {
                        self.state.pending_deletes.push(tombstone_key.clone());
                    }
                    self.current_remote.items.remove(&tombstone_key);
                }
                self.state.last_local.remove(&tombstone_key);
            }
            self.save_state()?;
            return Ok(());
        }

        for key in &keys {
            if self.current_remote.items.contains_key(key) {
                let bytes = self
                    .client
                    .get(key)
                    .await?
                    .ok_or_else(|| anyhow!("Remote item disappeared: {}", key))?;
                self.write_local_file(key, &bytes)?;
                let meta = self.meta_for_key(key, &bytes)?;
                self.state.last_local.insert(key.clone(), meta);
            } else if self.local_path_for_key(key)?.exists() {
                self.remove_local_file(key)?;
                self.state.last_local.remove(key);
            }
        }
        if remote_tombstone {
            let bytes = match self.client.get(&tombstone_key).await? {
                Some(bytes) => bytes,
                None => serde_json::to_vec(&serde_json::json!({
                    "book_id": id,
                    "deleted_at": now_secs(),
                    "device_id": self.state.device_id,
                }))?,
            };
            self.write_local_file(&tombstone_key, &bytes)?;
            let meta = self.meta_for_key(&tombstone_key, &bytes)?;
            self.state.last_local.insert(tombstone_key.clone(), meta);
        } else {
            if local_tombstone_path.exists() {
                std::fs::remove_file(&local_tombstone_path)?;
            }
            self.state.last_local.remove(&tombstone_key);
        }
        self.save_state()?;
        Ok(())
    }

    fn finalize_local_state(&mut self) -> anyhow::Result<()> {
        let tombstone_ids: HashSet<String> = self
            .state
            .last_local
            .keys()
            .filter_map(|k| {
                let prefix = "tombstones/";
                if let Some(rest) = k.strip_prefix(prefix) {
                    rest.strip_suffix(".json").map(|s| s.to_string())
                } else {
                    None
                }
            })
            .collect();
        if !tombstone_ids.is_empty() {
            let mut library = self.store.load_library_checked()?;
            let before = library.len();
            library.retain(|entry| !tombstone_ids.contains(&entry.id));
            if library.len() != before {
                self.store.save_library(&library)?;
            }
        }

        let mut settings = self.store.load_settings();
        let managed_bg = find_managed_background(&self.store.paths);
        match managed_bg {
            Some(path) => {
                if settings.custom_bg_image.as_deref() != Some(path.to_str().unwrap_or("")) {
                    settings.custom_bg_image = Some(path.to_string_lossy().to_string());
                }
            }
            None => {
                if settings
                    .custom_bg_image
                    .as_deref()
                    .map(|p| p.starts_with(self.store.paths.assets_dir.to_string_lossy().as_ref()))
                    .unwrap_or(false)
                {
                    settings.custom_bg_image = None;
                }
            }
        }
        self.store.save_settings(&settings)?;
        Ok(())
    }

    async fn upload_final_manifest(&mut self) -> anyhow::Result<()> {
        let final_local = self.build_local_snapshot()?;
        let mut remote_items = self.current_remote.items.clone();

        for key in self.state.pending_deletes.clone() {
            remote_items.remove(&key);
        }

        let tombstoned: HashSet<String> = final_local
            .items
            .keys()
            .filter_map(|k| {
                let prefix = "tombstones/";
                if let Some(rest) = k.strip_prefix(prefix) {
                    rest.strip_suffix(".json").map(|s| s.to_string())
                } else {
                    None
                }
            })
            .collect();
        let remote_keys: Vec<String> = remote_items.keys().cloned().collect();
        for key in remote_keys {
            if key.starts_with("tombstones/") {
                continue;
            }
            if let Some(id) = book_id_from_key(&key) {
                if tombstoned.contains(id) {
                    remote_items.remove(&key);
                }
            }
        }

        for (key, meta) in final_local.items.iter() {
            if !validate_remote_key(key) {
                continue;
            }
            if !key.starts_with("tombstones/") {
                if let Some(id) = book_id_from_key(key) {
                    if tombstoned.contains(id) {
                        continue;
                    }
                }
            }
            if self.state.pending_deletes.contains(key) {
                continue;
            }
            let needs_upload = match remote_items.get(key) {
                Some(remote_meta) => remote_meta.sha256 != meta.sha256,
                None => true,
            };
            if needs_upload {
                let bytes = self.bytes_for_key(key)?;
                self.client.put(key, bytes).await?;
                remote_items.insert(key.clone(), meta.clone());
            }
        }

        let manifest = Manifest {
            version: self.current_remote.version.max(1),
            updated_at: now_secs(),
            items: remote_items,
        };
        let bytes = serde_json::to_vec(&manifest)?;
        self.client.put(MANIFEST_FILE, bytes).await?;

        self.state.last_local = final_local.items;
        self.state.last_remote = manifest;
        self.state.pending_deletes.clear();
        self.save_state()?;
        Ok(())
    }

    fn bytes_for_key(&self, key: &str) -> anyhow::Result<Vec<u8>> {
        if key == "settings.json" {
            return syncable_settings_bytes(&self.store.load_settings());
        }
        let path = self.local_path_for_key(key)?;
        std::fs::read(&path).with_context(|| format!("failed to read local item {}", key))
    }

    fn meta_for_key(&self, key: &str, bytes: &[u8]) -> anyhow::Result<FileMeta> {
        let size = bytes.len() as u64;
        let mtime = self
            .local_path_for_key(key)
            .ok()
            .and_then(|p| std::fs::metadata(&p).ok())
            .map(|m| modified_ms(&m))
            .unwrap_or(0);
        let updated_at = if mtime == 0 { now_secs() } else { mtime };
        Ok(FileMeta {
            sha256: hash_bytes(bytes),
            size,
            updated_at,
            device_id: self.state.device_id.clone(),
        })
    }

    fn local_path_for_key(&self, key: &str) -> anyhow::Result<PathBuf> {
        if !validate_remote_key(key) {
            bail!("invalid sync key: {}", key);
        }
        if key == "library.json" {
            return Ok(self.store.paths.library_file.clone());
        }
        if key == "settings.json" {
            return Ok(self.store.paths.settings_file.clone());
        }
        if let Some(ext) = key.strip_prefix("custom_bg.") {
            return Ok(self.store.paths.custom_bg_path(ext));
        }
        let parts: Vec<&str> = key.split('/').collect();
        if parts.len() != 2 {
            bail!("invalid sync key: {}", key);
        }
        let (dir, file) = (parts[0], parts[1]);
        let id = file.split('.').next().unwrap_or_default();
        match dir {
            "books" => {
                let ext = if file.ends_with(".txt") { "txt" } else { "epub" };
                Ok(self.store.paths.book_path(id, ext))
            }
            "covers" => Ok(self.store.paths.cover_path(id)),
            "progress" => Ok(self.store.paths.progress_path(id)),
            "tombstones" => Ok(self.store.paths.tombstone_path(id)),
            _ => bail!("invalid sync key: {}", key),
        }
    }

    fn write_local_file(&self, key: &str, bytes: &[u8]) -> anyhow::Result<()> {
        if key == "settings.json" {
            let remote: AppSettings = serde_json::from_slice(bytes).context("invalid remote settings")?;
            let current = self.store.load_settings();
            let mut merged = remote;
            merged.data_dir = current.data_dir;
            merged.custom_bg_image = current.custom_bg_image;
            return self.store.save_settings(&merged);
        }

        let path = self.local_path_for_key(key)?;
        atomic_write(&path, bytes)?;
        if key.starts_with("custom_bg.") {
            clean_managed_backgrounds(&self.store.paths, Some(&path))?;
        }
        if let Some(id) = book_id_from_key(key) {
            invalidate_book_caches(&path, id);
        }
        Ok(())
    }

    fn remove_local_file(&self, key: &str) -> anyhow::Result<()> {
        let path = self.local_path_for_key(key)?;
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        if key.starts_with("custom_bg.") {
            clean_managed_backgrounds(&self.store.paths, None)?;
        }
        if let Some(id) = book_id_from_key(key) {
            invalidate_book_caches(&path, id);
        }
        Ok(())
    }

    fn save_state(&self) -> anyhow::Result<()> {
        crate::sync::state::save(&self.store.paths, &self.state)
    }
}

fn changed(current: Option<&FileMeta>, previous: Option<&FileMeta>) -> bool {
    match (current, previous) {
        (Some(current), Some(previous)) => current.sha256 != previous.sha256,
        (Some(_), None) => true,
        (None, Some(_)) => true,
        (None, None) => false,
    }
}

fn syncable_settings_bytes(settings: &AppSettings) -> anyhow::Result<Vec<u8>> {
    let mut remote = settings.clone();
    remote.data_dir = None;
    remote.custom_bg_image = None;
    Ok(serde_json::to_vec(&remote)?)
}

fn add_dir_items<F>(
    dir: &Path,
    items: &mut HashMap<String, FileMeta>,
    make_key: F,
    last_local: &HashMap<String, FileMeta>,
) -> anyhow::Result<()>
where
    F: Fn(&str, &str, &str) -> Option<String>,
{
    if !dir.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let Some((stem, ext)) = name.rsplit_once('.') else {
            continue;
        };
        if !valid_id(stem) {
            continue;
        }
        let Some(key) = make_key(&name, stem, ext) else {
            continue;
        };
        if !validate_remote_key(&key) {
            continue;
        }
        let metadata = std::fs::metadata(entry.path())?;
        let size = metadata.len();
        let mtime = modified_ms(&metadata);
        let meta = if let Some(prev) = last_local.get(&key) {
            if prev.size == size && prev.updated_at == mtime {
                prev.clone()
            } else {
                FileMeta {
                    sha256: hash_file(&entry.path())?,
                    size,
                    updated_at: mtime,
                    device_id: String::new(),
                }
            }
        } else {
            FileMeta {
                sha256: hash_file(&entry.path())?,
                size,
                updated_at: mtime,
                device_id: String::new(),
            }
        };
        items.insert(key, meta);
    }
    Ok(())
}

fn validate_remote_key(key: &str) -> bool {
    if key == "library.json" || key == "settings.json" {
        return true;
    }
    if let Some(ext) = key.strip_prefix("custom_bg.") {
        return VALID_IMAGE_EXTS.contains(&ext);
    }
    let Some((dir, file)) = key.split_once('/') else {
        return false;
    };
    if file.contains('/') || file.contains('\\') || file.contains("..") {
        return false;
    }
    let Some((id, ext)) = file.rsplit_once('.') else {
        return false;
    };
    if !valid_id(id) {
        return false;
    }
    match dir {
        "books" => ext == "epub" || ext == "txt",
        "covers" => ext == "jpg",
        "progress" | "tombstones" => ext == "json",
        _ => false,
    }
}

fn valid_id(id: &str) -> bool {
    crate::storage::paths::is_safe_book_id(id)
}

fn book_id_from_key(key: &str) -> Option<&str> {
    for prefix in ["books/", "covers/", "progress/", "tombstones/"] {
        if let Some(rest) = key.strip_prefix(prefix) {
            return rest.split('.').next();
        }
    }
    None
}

fn tombstone_timestamp(dir: &Path, key: &str) -> Option<u64> {
    let file = key.strip_prefix("tombstones/")?;
    let path = dir.join(file);
    let text = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    value.get("deleted_at").and_then(|v| v.as_u64())
}

fn hash_file(path: &Path) -> anyhow::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn modified_ms(metadata: &std::fs::Metadata) -> u64 {
    metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn atomic_write(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
    std::fs::write(&tmp, bytes)?;
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn atomic_copy(source: &Path, dest: &Path) -> anyhow::Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = dest.with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
    std::fs::copy(source, &tmp)?;
    if dest.exists() {
        std::fs::remove_file(dest)?;
    }
    std::fs::rename(&tmp, dest)?;
    Ok(())
}

fn invalidate_book_caches(path: &Path, book_id: &str) {
    EpubParser::invalidate(path);
    TxtParser::invalidate(path);
    crate::commands::reader::invalidate_book(book_id);
    crate::clear_epub_asset_cache();
}

fn find_managed_background(paths: &AppPaths) -> Option<PathBuf> {
    if !paths.assets_dir.exists() {
        return None;
    }
    let mut candidates: Vec<PathBuf> = std::fs::read_dir(&paths.assets_dir)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("custom_bg."))
                .unwrap_or(false)
        })
        .collect();
    candidates.sort();
    candidates.into_iter().next()
}

fn clean_managed_backgrounds(paths: &AppPaths, keep: Option<&Path>) -> anyhow::Result<()> {
    if !paths.assets_dir.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(&paths.assets_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("custom_bg.") {
            continue;
        }
        let path = entry.path();
        if keep.map(|k| k == path).unwrap_or(false) {
            continue;
        }
        if let Err(e) = std::fs::remove_file(&path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(e.into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::store::Store;
    use crate::sync::webdav::WebDavClient;
    use async_trait::async_trait;
    use std::sync::Arc;

    #[derive(Clone)]
    struct FakeWebDav {
        files: Arc<Mutex<HashMap<String, Vec<u8>>>>,
        fail_manifest_put: Arc<Mutex<bool>>,
    }

    impl FakeWebDav {
        fn with_files(files: Arc<Mutex<HashMap<String, Vec<u8>>>>) -> Self {
            Self {
                files,
                fail_manifest_put: Arc::new(Mutex::new(false)),
            }
        }
    }

    #[async_trait]
    impl WebDavClient for FakeWebDav {
        async fn ensure_root(&self) -> anyhow::Result<()> {
            Ok(())
        }

        async fn get(&self, path: &str) -> anyhow::Result<Option<Vec<u8>>> {
            Ok(self.files.lock().unwrap().get(path).cloned())
        }

        async fn put(&self, path: &str, bytes: Vec<u8>) -> anyhow::Result<()> {
            if path == MANIFEST_FILE && *self.fail_manifest_put.lock().unwrap() {
                bail!("manifest upload failed");
            }
            self.files.lock().unwrap().insert(path.to_string(), bytes);
            Ok(())
        }

        async fn delete(&self, path: &str) -> anyhow::Result<()> {
            self.files.lock().unwrap().remove(path);
            Ok(())
        }
    }

    fn temp_paths() -> (AppPaths, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "epubreader-sync-test-{}",
            uuid::Uuid::new_v4()
        ));
        let paths = AppPaths::new(Some(dir.clone()));
        paths.ensure_dirs().unwrap();
        (paths, dir)
    }

    fn seed_local_book(paths: &AppPaths, id: &str, content: &[u8]) {
        let book = paths.book_path(id, "epub");
        std::fs::write(&book, content).unwrap();
        let library = serde_json::json!([{
            "id": id,
            "title": "Test Book",
            "author": "Author",
            "cover": null,
            "file_path": book.to_string_lossy(),
            "added_at": 1,
            "file_size": content.len(),
            "format": "epub"
        }]);
        std::fs::write(&paths.library_file, serde_json::to_vec(&library).unwrap()).unwrap();
        let settings = serde_json::json!({
            "theme": "light",
            "font_size": 18.0,
            "line_height": 1.8,
            "font_family": "sans-serif",
            "custom_bg_image": null,
            "data_dir": null,
            "close_behavior": "quit"
        });
        std::fs::write(&paths.settings_file, serde_json::to_vec(&settings).unwrap()).unwrap();
    }

    fn replace_remote_file(
        files: &Arc<Mutex<HashMap<String, Vec<u8>>>>,
        key: &str,
        bytes: Vec<u8>,
        device: &str,
    ) {
        let mut manifest: Manifest = files
            .lock()
            .unwrap()
            .get(MANIFEST_FILE)
            .and_then(|b| serde_json::from_slice(b).ok())
            .unwrap_or_default();
        manifest.items.insert(
            key.to_string(),
            FileMeta {
                sha256: hash_bytes(&bytes),
                size: bytes.len() as u64,
                updated_at: now_secs(),
                device_id: device.to_string(),
            },
        );
        files
            .lock()
            .unwrap()
            .insert(key.to_string(), bytes);
        files.lock().unwrap().insert(
            MANIFEST_FILE.to_string(),
            serde_json::to_vec(&manifest).unwrap(),
        );
    }

    #[tokio::test]
    async fn first_sync_uploads_then_second_sync_is_noop() {
        let (paths, dir) = temp_paths();
        seed_local_book(&paths, "book-1", b"epub-content");
        let files = Arc::new(Mutex::new(HashMap::new()));
        let mut engine =
            SyncEngine::new(paths.clone(), Box::new(FakeWebDav::with_files(Arc::clone(&files))))
                .unwrap();

        let preview = engine.preview().await.unwrap();
        assert!(!preview.actions.is_empty());
        engine.apply(&[], |_, _, _| {}).await.unwrap();

        assert!(files.lock().unwrap().contains_key("books/book-1.epub"));
        assert!(files.lock().unwrap().contains_key("library.json"));
        assert!(files.lock().unwrap().contains_key("settings.json"));

        let mut second_engine = SyncEngine::new(
            paths.clone(),
            Box::new(FakeWebDav::with_files(Arc::clone(&files))),
        )
        .unwrap();
        let second_preview = second_engine.preview().await.unwrap();
        assert!(second_preview.actions.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn local_edit_uploads_on_next_sync() {
        let (paths, dir) = temp_paths();
        seed_local_book(&paths, "book-1", b"epub-content");
        let files = Arc::new(Mutex::new(HashMap::new()));
        let mut engine =
            SyncEngine::new(paths.clone(), Box::new(FakeWebDav::with_files(Arc::clone(&files))))
                .unwrap();
        engine.preview().await.unwrap();
        engine.apply(&[], |_, _, _| {}).await.unwrap();

        std::fs::write(paths.book_path("book-1", "epub"), b"epub-content-v2").unwrap();
        let mut engine = SyncEngine::new(
            paths.clone(),
            Box::new(FakeWebDav::with_files(Arc::clone(&files))),
        )
        .unwrap();
        let preview = engine.preview().await.unwrap();
        assert!(preview.actions.iter().any(|a| {
            a.id == "books/book-1.epub" && a.kind == ActionKind::Upload
        }));
        engine.apply(&[], |_, _, _| {}).await.unwrap();
        assert_eq!(
            files.lock().unwrap().get("books/book-1.epub").unwrap(),
            b"epub-content-v2"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn remote_edit_downloads_on_next_sync() {
        let (paths, dir) = temp_paths();
        seed_local_book(&paths, "book-1", b"epub-content");
        let files = Arc::new(Mutex::new(HashMap::new()));
        let mut engine =
            SyncEngine::new(paths.clone(), Box::new(FakeWebDav::with_files(Arc::clone(&files))))
                .unwrap();
        engine.preview().await.unwrap();
        engine.apply(&[], |_, _, _| {}).await.unwrap();

        replace_remote_file(&files, "books/book-1.epub", b"remote-content".to_vec(), "device-b");
        let mut engine = SyncEngine::new(
            paths.clone(),
            Box::new(FakeWebDav::with_files(Arc::clone(&files))),
        )
        .unwrap();
        let preview = engine.preview().await.unwrap();
        assert!(preview.actions.iter().any(|a| {
            a.id == "books/book-1.epub" && a.kind == ActionKind::Download
        }));
        engine.apply(&[], |_, _, _| {}).await.unwrap();
        assert_eq!(
            std::fs::read(paths.book_path("book-1", "epub")).unwrap(),
            b"remote-content"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn bilateral_edit_is_a_conflict_and_needs_decision() {
        let (paths, dir) = temp_paths();
        seed_local_book(&paths, "book-1", b"epub-content");
        let files = Arc::new(Mutex::new(HashMap::new()));
        let mut engine =
            SyncEngine::new(paths.clone(), Box::new(FakeWebDav::with_files(Arc::clone(&files))))
                .unwrap();
        engine.preview().await.unwrap();
        engine.apply(&[], |_, _, _| {}).await.unwrap();

        std::fs::write(paths.book_path("book-1", "epub"), b"local-v2").unwrap();
        replace_remote_file(&files, "books/book-1.epub", b"remote-v2".to_vec(), "device-b");
        let mut engine = SyncEngine::new(
            paths.clone(),
            Box::new(FakeWebDav::with_files(Arc::clone(&files))),
        )
        .unwrap();
        let preview = engine.preview().await.unwrap();
        let conflict = preview
            .actions
            .iter()
            .find(|a| a.id == "books/book-1.epub" && a.kind == ActionKind::Conflict)
            .expect("conflict expected");
        assert_eq!(conflict.conflict_type.as_deref(), Some("content"));
        assert!(engine.apply(&[], |_, _, _| {}).await.is_err());

        engine
            .apply(
                &[SyncDecision {
                    id: "books/book-1.epub".to_string(),
                    choice: "local".to_string(),
                }],
                |_, _, _| {},
            )
            .await
            .unwrap();
        assert_eq!(
            files.lock().unwrap().get("books/book-1.epub").unwrap(),
            b"local-v2"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn delete_propagates_tombstone_both_directions() {
        let (paths_a, dir_a) = temp_paths();
        seed_local_book(&paths_a, "book-1", b"epub-content");
        let files = Arc::new(Mutex::new(HashMap::new()));
        let mut engine =
            SyncEngine::new(paths_a.clone(), Box::new(FakeWebDav::with_files(Arc::clone(&files))))
                .unwrap();
        engine.preview().await.unwrap();
        engine.apply(&[], |_, _, _| {}).await.unwrap();

        let store = Store::new(Some(paths_a.data_dir.clone()));
        store.remove_book("book-1").unwrap();

        let mut engine_a = SyncEngine::new(
            paths_a.clone(),
            Box::new(FakeWebDav::with_files(Arc::clone(&files))),
        )
        .unwrap();
        let preview = engine_a.preview().await.unwrap();
        assert!(preview.actions.iter().any(|a| {
            a.id == "books/book-1.epub" && a.kind == ActionKind::Delete
        }));
        assert!(preview.actions.iter().any(|a| {
            a.id == "tombstones/book-1.json" && a.kind == ActionKind::Upload
        }));
        engine_a.apply(&[], |_, _, _| {}).await.unwrap();
        assert!(files.lock().unwrap().contains_key("tombstones/book-1.json"));
        assert!(!files.lock().unwrap().contains_key("books/book-1.epub"));

        let (paths_b, dir_b) = temp_paths();
        let mut engine_b = SyncEngine::new(
            paths_b.clone(),
            Box::new(FakeWebDav::with_files(Arc::clone(&files))),
        )
        .unwrap();
        let preview_b = engine_b.preview().await.unwrap();
        assert!(preview_b.actions.iter().any(|a| {
            a.id == "tombstones/book-1.json" && a.kind == ActionKind::Download
        }));
        engine_b.apply(&[], |_, _, _| {}).await.unwrap();
        assert!(!paths_b.book_path("book-1", "epub").exists());
        let library = Store::new(Some(paths_b.data_dir.clone()))
            .load_library_checked()
            .unwrap();
        assert!(library.iter().all(|e| e.id != "book-1"));

        let _ = std::fs::remove_dir_all(&dir_a);
        let _ = std::fs::remove_dir_all(&dir_b);
    }

    #[tokio::test]
    async fn delete_vs_modify_conflict_can_keep_remote_deletion() {
        let (paths_a, dir_a) = temp_paths();
        seed_local_book(&paths_a, "book-1", b"epub-content");
        let files = Arc::new(Mutex::new(HashMap::new()));
        let mut engine_a =
            SyncEngine::new(paths_a.clone(), Box::new(FakeWebDav::with_files(Arc::clone(&files))))
                .unwrap();
        engine_a.preview().await.unwrap();
        engine_a.apply(&[], |_, _, _| {}).await.unwrap();

        Store::new(Some(paths_a.data_dir.clone()))
            .remove_book("book-1")
            .unwrap();
        let mut engine_a2 = SyncEngine::new(
            paths_a.clone(),
            Box::new(FakeWebDav::with_files(Arc::clone(&files))),
        )
        .unwrap();
        engine_a2.preview().await.unwrap();
        engine_a2.apply(&[], |_, _, _| {}).await.unwrap();
        assert!(files.lock().unwrap().contains_key("tombstones/book-1.json"));
        assert!(!files.lock().unwrap().contains_key("books/book-1.epub"));

        let (paths_b, dir_b) = temp_paths();
        seed_local_book(&paths_b, "book-1", b"local-v2");
        let mut engine_b = SyncEngine::new(
            paths_b.clone(),
            Box::new(FakeWebDav::with_files(Arc::clone(&files))),
        )
        .unwrap();
        let preview = engine_b.preview().await.unwrap();
        let conflict = preview
            .actions
            .iter()
            .find(|a| a.id == "books/book-1.epub" && a.kind == ActionKind::Conflict)
            .expect("delete vs modify conflict expected");
        assert_eq!(conflict.conflict_type.as_deref(), Some("delete_vs_modify"));

        let decisions: Vec<SyncDecision> = preview
            .actions
            .iter()
            .filter(|a| a.kind == ActionKind::Conflict)
            .map(|a| SyncDecision {
                id: a.id.clone(),
                choice: "remote".to_string(),
            })
            .collect();
        engine_b.apply(&decisions, |_, _, _| {}).await.unwrap();
        assert!(!paths_b.book_path("book-1", "epub").exists());
        assert!(paths_b.tombstone_path("book-1").exists());
        let library = Store::new(Some(paths_b.data_dir.clone()))
            .load_library_checked()
            .unwrap();
        assert!(library.iter().all(|entry| entry.id != "book-1"));

        let _ = std::fs::remove_dir_all(&dir_a);
        let _ = std::fs::remove_dir_all(&dir_b);
    }

    #[tokio::test]
    async fn delete_vs_modify_conflict_can_keep_local_edit() {
        let (paths_a, dir_a) = temp_paths();
        seed_local_book(&paths_a, "book-1", b"epub-content");
        let files = Arc::new(Mutex::new(HashMap::new()));
        let mut engine_a = SyncEngine::new(
            paths_a.clone(),
            Box::new(FakeWebDav::with_files(Arc::clone(&files))),
        )
        .unwrap();
        engine_a.preview().await.unwrap();
        engine_a.apply(&[], |_, _, _| {}).await.unwrap();

        let (paths_b, dir_b) = temp_paths();
        let mut engine_b = SyncEngine::new(
            paths_b.clone(),
            Box::new(FakeWebDav::with_files(Arc::clone(&files))),
        )
        .unwrap();
        engine_b.preview().await.unwrap();
        engine_b.apply(&[], |_, _, _| {}).await.unwrap();
        assert!(paths_b.book_path("book-1", "epub").exists());

        Store::new(Some(paths_b.data_dir.clone()))
            .remove_book("book-1")
            .unwrap();
        let mut engine_b2 = SyncEngine::new(
            paths_b.clone(),
            Box::new(FakeWebDav::with_files(Arc::clone(&files))),
        )
        .unwrap();
        engine_b2.preview().await.unwrap();
        engine_b2.apply(&[], |_, _, _| {}).await.unwrap();
        assert!(files.lock().unwrap().contains_key("tombstones/book-1.json"));
        assert!(!files.lock().unwrap().contains_key("books/book-1.epub"));

        std::fs::write(paths_a.book_path("book-1", "epub"), b"local-v2").unwrap();
        let mut engine_a2 = SyncEngine::new(
            paths_a.clone(),
            Box::new(FakeWebDav::with_files(Arc::clone(&files))),
        )
        .unwrap();
        let preview = engine_a2.preview().await.unwrap();
        let conflict = preview
            .actions
            .iter()
            .find(|a| a.id == "books/book-1.epub" && a.kind == ActionKind::Conflict)
            .expect("delete vs modify conflict expected");
        assert_eq!(conflict.conflict_type.as_deref(), Some("delete_vs_modify"));

        let decisions: Vec<SyncDecision> = preview
            .actions
            .iter()
            .filter(|a| a.kind == ActionKind::Conflict)
            .map(|a| SyncDecision {
                id: a.id.clone(),
                choice: "local".to_string(),
            })
            .collect();
        engine_a2.apply(&decisions, |_, _, _| {}).await.unwrap();
        assert_eq!(
            files.lock().unwrap().get("books/book-1.epub").unwrap(),
            b"local-v2"
        );
        assert!(!files.lock().unwrap().contains_key("tombstones/book-1.json"));

        let mut engine_a3 = SyncEngine::new(
            paths_a.clone(),
            Box::new(FakeWebDav::with_files(Arc::clone(&files))),
        )
        .unwrap();
        let second_preview = engine_a3.preview().await.unwrap();
        assert!(
            !second_preview
                .actions
                .iter()
                .any(|a| a.id == "tombstones/book-1.json")
        );

        let _ = std::fs::remove_dir_all(&dir_a);
        let _ = std::fs::remove_dir_all(&dir_b);
    }

    #[tokio::test]
    async fn delete_vs_modify_conflict_can_keep_local_deletion() {
        let (paths_a, dir_a) = temp_paths();
        seed_local_book(&paths_a, "book-1", b"epub-content");
        let files = Arc::new(Mutex::new(HashMap::new()));
        let mut engine_a = SyncEngine::new(
            paths_a.clone(),
            Box::new(FakeWebDav::with_files(Arc::clone(&files))),
        )
        .unwrap();
        engine_a.preview().await.unwrap();
        engine_a.apply(&[], |_, _, _| {}).await.unwrap();

        let (paths_b, dir_b) = temp_paths();
        let mut engine_b = SyncEngine::new(
            paths_b.clone(),
            Box::new(FakeWebDav::with_files(Arc::clone(&files))),
        )
        .unwrap();
        engine_b.preview().await.unwrap();
        engine_b.apply(&[], |_, _, _| {}).await.unwrap();
        Store::new(Some(paths_b.data_dir.clone()))
            .remove_book("book-1")
            .unwrap();

        std::fs::write(paths_a.book_path("book-1", "epub"), b"remote-v2").unwrap();
        let mut engine_a2 = SyncEngine::new(
            paths_a.clone(),
            Box::new(FakeWebDav::with_files(Arc::clone(&files))),
        )
        .unwrap();
        engine_a2.preview().await.unwrap();
        engine_a2.apply(&[], |_, _, _| {}).await.unwrap();
        assert_eq!(
            files.lock().unwrap().get("books/book-1.epub").unwrap(),
            b"remote-v2"
        );

        let mut engine_b2 = SyncEngine::new(
            paths_b.clone(),
            Box::new(FakeWebDav::with_files(Arc::clone(&files))),
        )
        .unwrap();
        let preview = engine_b2.preview().await.unwrap();
        let conflict = preview
            .actions
            .iter()
            .find(|a| a.id == "books/book-1.epub" && a.kind == ActionKind::Conflict)
            .expect("delete vs modify conflict expected");
        assert_eq!(conflict.conflict_type.as_deref(), Some("delete_vs_modify"));
        assert!(!conflict.local_exists);
        assert!(conflict.remote_exists);

        let decisions: Vec<SyncDecision> = preview
            .actions
            .iter()
            .filter(|a| a.kind == ActionKind::Conflict)
            .map(|a| SyncDecision {
                id: a.id.clone(),
                choice: "local".to_string(),
            })
            .collect();
        engine_b2.apply(&decisions, |_, _, _| {}).await.unwrap();
        assert!(!files.lock().unwrap().contains_key("books/book-1.epub"));
        assert!(files.lock().unwrap().contains_key("tombstones/book-1.json"));
        assert!(!paths_b.book_path("book-1", "epub").exists());
        assert!(paths_b.tombstone_path("book-1").exists());

        let mut engine_b3 = SyncEngine::new(
            paths_b.clone(),
            Box::new(FakeWebDav::with_files(Arc::clone(&files))),
        )
        .unwrap();
        let second_preview = engine_b3.preview().await.unwrap();
        assert!(
            !second_preview
                .actions
                .iter()
                .any(|a| a.id == "tombstones/book-1.json")
        );
        assert!(
            !second_preview
                .actions
                .iter()
                .any(|a| a.id == "books/book-1.epub")
        );

        let _ = std::fs::remove_dir_all(&dir_a);
        let _ = std::fs::remove_dir_all(&dir_b);
    }

    #[tokio::test]
    async fn delete_vs_modify_local_choice_keeps_whole_book() {
        let (paths_a, dir_a) = temp_paths();
        seed_local_book(&paths_a, "book-1", b"epub-content");
        std::fs::write(paths_a.cover_path("book-1"), b"cover").unwrap();
        std::fs::write(paths_a.progress_path("book-1"), b"progress").unwrap();
        let files = Arc::new(Mutex::new(HashMap::new()));
        let mut engine_a = SyncEngine::new(
            paths_a.clone(),
            Box::new(FakeWebDav::with_files(Arc::clone(&files))),
        )
        .unwrap();
        engine_a.preview().await.unwrap();
        engine_a.apply(&[], |_, _, _| {}).await.unwrap();

        let (paths_b, dir_b) = temp_paths();
        let mut engine_b = SyncEngine::new(
            paths_b.clone(),
            Box::new(FakeWebDav::with_files(Arc::clone(&files))),
        )
        .unwrap();
        engine_b.preview().await.unwrap();
        engine_b.apply(&[], |_, _, _| {}).await.unwrap();

        Store::new(Some(paths_b.data_dir.clone()))
            .remove_book("book-1")
            .unwrap();
        let mut engine_b2 = SyncEngine::new(
            paths_b.clone(),
            Box::new(FakeWebDav::with_files(Arc::clone(&files))),
        )
        .unwrap();
        engine_b2.preview().await.unwrap();
        engine_b2.apply(&[], |_, _, _| {}).await.unwrap();
        assert!(files.lock().unwrap().contains_key("tombstones/book-1.json"));
        assert!(!files.lock().unwrap().contains_key("covers/book-1.jpg"));
        assert!(!files.lock().unwrap().contains_key("progress/book-1.json"));

        std::fs::write(paths_a.book_path("book-1", "epub"), b"local-v2").unwrap();
        let mut engine_a2 = SyncEngine::new(
            paths_a.clone(),
            Box::new(FakeWebDav::with_files(Arc::clone(&files))),
        )
        .unwrap();
        let preview = engine_a2.preview().await.unwrap();
        let conflict = preview
            .actions
            .iter()
            .find(|a| a.id == "books/book-1.epub" && a.kind == ActionKind::Conflict)
            .expect("delete vs modify conflict expected");
        assert_eq!(conflict.conflict_type.as_deref(), Some("delete_vs_modify"));
        assert!(
            !preview
                .actions
                .iter()
                .any(|a| a.id.starts_with("covers/") || a.id.starts_with("progress/"))
        );

        let decisions: Vec<SyncDecision> = preview
            .actions
            .iter()
            .filter(|a| a.kind == ActionKind::Conflict)
            .map(|a| SyncDecision {
                id: a.id.clone(),
                choice: "local".to_string(),
            })
            .collect();
        engine_a2.apply(&decisions, |_, _, _| {}).await.unwrap();

        assert!(paths_a.cover_path("book-1").exists());
        assert!(paths_a.progress_path("book-1").exists());
        assert!(!files.lock().unwrap().contains_key("tombstones/book-1.json"));
        assert_eq!(
            files.lock().unwrap().get("books/book-1.epub").unwrap(),
            b"local-v2"
        );
        assert_eq!(
            files.lock().unwrap().get("covers/book-1.jpg").unwrap(),
            b"cover"
        );
        assert_eq!(
            files.lock().unwrap().get("progress/book-1.json").unwrap(),
            b"progress"
        );
        let library = Store::new(Some(paths_a.data_dir.clone()))
            .load_library_checked()
            .unwrap();
        assert!(library.iter().any(|entry| entry.id == "book-1"));

        let _ = std::fs::remove_dir_all(&dir_a);
        let _ = std::fs::remove_dir_all(&dir_b);
    }

    #[tokio::test]
    async fn delete_vs_modify_remote_choice_keeps_whole_book() {
        let (paths_a, dir_a) = temp_paths();
        seed_local_book(&paths_a, "book-1", b"epub-content");
        std::fs::write(paths_a.cover_path("book-1"), b"cover").unwrap();
        std::fs::write(paths_a.progress_path("book-1"), b"progress").unwrap();
        let files = Arc::new(Mutex::new(HashMap::new()));
        let mut engine_a = SyncEngine::new(
            paths_a.clone(),
            Box::new(FakeWebDav::with_files(Arc::clone(&files))),
        )
        .unwrap();
        engine_a.preview().await.unwrap();
        engine_a.apply(&[], |_, _, _| {}).await.unwrap();

        let (paths_b, dir_b) = temp_paths();
        let mut engine_b = SyncEngine::new(
            paths_b.clone(),
            Box::new(FakeWebDav::with_files(Arc::clone(&files))),
        )
        .unwrap();
        engine_b.preview().await.unwrap();
        engine_b.apply(&[], |_, _, _| {}).await.unwrap();

        Store::new(Some(paths_b.data_dir.clone()))
            .remove_book("book-1")
            .unwrap();

        std::fs::write(paths_a.book_path("book-1", "epub"), b"remote-v2").unwrap();
        let mut engine_a2 = SyncEngine::new(
            paths_a.clone(),
            Box::new(FakeWebDav::with_files(Arc::clone(&files))),
        )
        .unwrap();
        engine_a2.preview().await.unwrap();
        engine_a2.apply(&[], |_, _, _| {}).await.unwrap();

        let mut engine_b2 = SyncEngine::new(
            paths_b.clone(),
            Box::new(FakeWebDav::with_files(Arc::clone(&files))),
        )
        .unwrap();
        let preview = engine_b2.preview().await.unwrap();
        let conflict = preview
            .actions
            .iter()
            .find(|a| a.id == "books/book-1.epub" && a.kind == ActionKind::Conflict)
            .expect("delete vs modify conflict expected");
        assert_eq!(conflict.conflict_type.as_deref(), Some("delete_vs_modify"));
        assert!(
            !preview
                .actions
                .iter()
                .any(|a| a.id.starts_with("covers/") || a.id.starts_with("progress/"))
        );

        let decisions: Vec<SyncDecision> = preview
            .actions
            .iter()
            .filter(|a| a.kind == ActionKind::Conflict)
            .map(|a| SyncDecision {
                id: a.id.clone(),
                choice: "remote".to_string(),
            })
            .collect();
        engine_b2.apply(&decisions, |_, _, _| {}).await.unwrap();

        assert!(paths_b.book_path("book-1", "epub").exists());
        assert!(paths_b.cover_path("book-1").exists());
        assert!(paths_b.progress_path("book-1").exists());
        assert!(!paths_b.tombstone_path("book-1").exists());
        assert!(!files.lock().unwrap().contains_key("tombstones/book-1.json"));
        assert_eq!(
            files.lock().unwrap().get("books/book-1.epub").unwrap(),
            b"remote-v2"
        );
        let library = Store::new(Some(paths_b.data_dir.clone()))
            .load_library_checked()
            .unwrap();
        assert!(library.iter().any(|entry| entry.id == "book-1"));

        let _ = std::fs::remove_dir_all(&dir_a);
        let _ = std::fs::remove_dir_all(&dir_b);
    }

    #[tokio::test]
    async fn manifest_failure_is_retryable() {
        let (paths, dir) = temp_paths();
        seed_local_book(&paths, "book-1", b"epub-content");
        let files = Arc::new(Mutex::new(HashMap::new()));
        let fail = Arc::new(Mutex::new(false));
        let client = FakeWebDav {
            files: Arc::clone(&files),
            fail_manifest_put: Arc::clone(&fail),
        };
        let mut engine = SyncEngine::new(paths.clone(), Box::new(client)).unwrap();
        engine.preview().await.unwrap();
        engine.apply(&[], |_, _, _| {}).await.unwrap();

        std::fs::write(paths.book_path("book-1", "epub"), b"v2").unwrap();
        *fail.lock().unwrap() = true;
        let client = FakeWebDav {
            files: Arc::clone(&files),
            fail_manifest_put: Arc::clone(&fail),
        };
        let mut engine = SyncEngine::new(paths.clone(), Box::new(client)).unwrap();
        engine.preview().await.unwrap();
        assert!(engine.apply(&[], |_, _, _| {}).await.is_err());

        *fail.lock().unwrap() = false;
        let preview = engine.preview().await.unwrap();
        assert!(preview
            .actions
            .iter()
            .any(|a| a.id == "books/book-1.epub"));
        engine.apply(&[], |_, _, _| {}).await.unwrap();
        assert_eq!(
            files.lock().unwrap().get("books/book-1.epub").unwrap(),
            b"v2"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn custom_background_is_copied_and_synced() {
        let (paths, dir) = temp_paths();
        seed_local_book(&paths, "book-1", b"epub-content");
        let external = dir.join("external-bg.png");
        std::fs::write(&external, b"bg-bytes").unwrap();
        let store = Store::new(Some(paths.data_dir.clone()));
        let mut settings = store.load_settings();
        settings.custom_bg_image = Some(external.to_string_lossy().to_string());
        store.save_settings(&settings).unwrap();

        let files = Arc::new(Mutex::new(HashMap::new()));
        let mut engine =
            SyncEngine::new(paths.clone(), Box::new(FakeWebDav::with_files(Arc::clone(&files))))
                .unwrap();
        engine.preview().await.unwrap();
        engine.apply(&[], |_, _, _| {}).await.unwrap();
        assert!(files.lock().unwrap().contains_key("custom_bg.png"));
        let settings_after = Store::new(Some(paths.data_dir.clone())).load_settings();
        assert!(
            settings_after
                .custom_bg_image
                .as_deref()
                .unwrap()
                .contains("assets")
        );

        let (paths_b, dir_b) = temp_paths();
        let mut engine_b = SyncEngine::new(
            paths_b.clone(),
            Box::new(FakeWebDav::with_files(Arc::clone(&files))),
        )
        .unwrap();
        engine_b.preview().await.unwrap();
        engine_b.apply(&[], |_, _, _| {}).await.unwrap();
        assert!(paths_b.assets_dir.join("custom_bg.png").exists());
        let settings_b = Store::new(Some(paths_b.data_dir.clone())).load_settings();
        assert!(
            settings_b
                .custom_bg_image
                .as_deref()
                .unwrap()
                .starts_with(paths_b.assets_dir.to_string_lossy().as_ref())
        );

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&dir_b);
    }

    #[tokio::test]
    async fn downloaded_settings_keep_local_data_dir() {
        let (paths, dir) = temp_paths();
        seed_local_book(&paths, "book-1", b"epub-content");
        let files = Arc::new(Mutex::new(HashMap::new()));
        let mut engine =
            SyncEngine::new(paths.clone(), Box::new(FakeWebDav::with_files(Arc::clone(&files))))
                .unwrap();
        engine.preview().await.unwrap();
        engine.apply(&[], |_, _, _| {}).await.unwrap();

        let remote_settings = serde_json::json!({
            "theme": "dark",
            "font_size": 20.0,
            "line_height": 2.0,
            "font_family": "sans-serif",
            "custom_bg_image": "D:/remote/bg.png",
            "data_dir": "D:/remote-data",
            "close_behavior": "minimize_to_tray"
        });
        replace_remote_file(
            &files,
            "settings.json",
            serde_json::to_vec(&remote_settings).unwrap(),
            "device-b",
        );

        let (paths_b, dir_b) = temp_paths();
        seed_local_book(&paths_b, "book-1", b"epub-content");
        let store_b = Store::new(Some(paths_b.data_dir.clone()));
        let mut local_settings = store_b.load_settings();
        local_settings.data_dir = Some("C:/local-data".to_string());
        local_settings.custom_bg_image = Some("C:/local-bg.png".to_string());
        store_b.save_settings(&local_settings).unwrap();

        let mut engine_b = SyncEngine::new(
            paths_b.clone(),
            Box::new(FakeWebDav::with_files(Arc::clone(&files))),
        )
        .unwrap();
        let initial_preview = engine_b.preview().await.unwrap();
        let initial_decisions: Vec<SyncDecision> = initial_preview
            .actions
            .iter()
            .filter(|a| a.kind == ActionKind::Conflict)
            .map(|a| SyncDecision {
                id: a.id.clone(),
                choice: "local".to_string(),
            })
            .collect();
        engine_b
            .apply(&initial_decisions, |_, _, _| {})
            .await
            .unwrap();

        replace_remote_file(
            &files,
            "settings.json",
            serde_json::to_vec(&remote_settings).unwrap(),
            "device-b",
        );

        let mut engine_b2 = SyncEngine::new(
            paths_b.clone(),
            Box::new(FakeWebDav::with_files(Arc::clone(&files))),
        )
        .unwrap();
        engine_b2.preview().await.unwrap();
        engine_b2.apply(&[], |_, _, _| {}).await.unwrap();

        let after = Store::new(Some(paths_b.data_dir.clone())).load_settings();
        assert_eq!(after.theme, "dark");
        assert_eq!(after.data_dir.as_deref(), Some("C:/local-data"));
        assert_eq!(after.custom_bg_image.as_deref(), Some("C:/local-bg.png"));

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&dir_b);
    }

    #[test]
    fn rejects_unsafe_remote_keys() {
        assert!(!validate_remote_key("../manifest.json"));
        assert!(!validate_remote_key("books/../library.json"));
        assert!(!validate_remote_key("books/..\\evil.epub"));
        assert!(!validate_remote_key("books/../../etc/passwd"));
        assert!(!validate_remote_key("tombstones/../x.json"));
        assert!(!validate_remote_key("custom_bg.exe"));
        assert!(validate_remote_key("books/book-1.epub"));
        assert!(validate_remote_key("tombstones/book-1.json"));
        assert!(validate_remote_key("custom_bg.png"));
    }
}
