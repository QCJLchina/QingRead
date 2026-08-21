use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::paths::AppPaths;
use crate::epub::parser::BookMetadata;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryEntry {
    pub id: String,
    pub title: String,
    pub author: String,
    pub cover: Option<String>,
    pub file_path: String,
    pub added_at: u64,
    pub file_size: u64,
    /// 文件格式：`epub` 或 `txt`，缺省为 `epub`（兼容老数据）
    #[serde(default = "default_format")]
    pub format: String,
}

fn default_format() -> String {
    "epub".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadingProgress {
    pub book_id: String,
    pub chapter_index: usize,
    pub page_in_chapter: usize,
    pub total_pages_read: usize,
    pub last_read: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub theme: String,
    pub font_size: f32,
    pub line_height: f32,
    pub font_family: String,
    pub custom_bg_image: Option<String>,
    #[serde(deserialize_with = "deserialize_optional_string")]
    pub data_dir: Option<String>,
    /// 关闭主窗口时的行为：`minimize_to_tray`（最小化到托盘）或 `quit`（直接退出）
    pub close_behavior: String,
}

/// 把空串视为 None，避免 settings.json 里残留 Some("")
fn deserialize_optional_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    Ok(opt.filter(|s| !s.is_empty()))
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "light".to_string(),
            font_size: 18.0,
            line_height: 1.8,
            font_family: "system-ui, -apple-system, sans-serif".to_string(),
            custom_bg_image: None,
            data_dir: None,
            close_behavior: "quit".to_string(),
        }
    }
}

pub struct Store {
    pub paths: AppPaths,
}

impl Store {
    pub fn new(data_dir: Option<PathBuf>) -> Self {
        let paths = AppPaths::new(data_dir);
        if let Err(e) = paths.ensure_dirs() {
            eprintln!("[Store::new] ensure_dirs failed: {e} (data_dir: {:?})", paths.data_dir);
        }
        Self { paths }
    }

    pub fn load_library_checked(&self) -> anyhow::Result<Vec<LibraryEntry>> {
        load_json_with_backup(&self.paths.library_file, "书库")
    }

    pub fn save_library(&self, library: &[LibraryEntry]) -> anyhow::Result<()> {
        if let Some(parent) = self.paths.library_file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        atomic_write_json(&self.paths.library_file, library)?;
        Ok(())
    }

    pub fn load_settings(&self) -> AppSettings {
        load_json_with_backup(&self.paths.settings_file, "设置").unwrap_or_default()
    }

    pub fn save_settings(&self, settings: &AppSettings) -> anyhow::Result<()> {
        if let Some(parent) = self.paths.settings_file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        atomic_write_json(&self.paths.settings_file, settings)?;
        Ok(())
    }

    pub fn load_progress(&self, book_id: &str) -> Option<ReadingProgress> {
        if validate_book_id(book_id).is_err() {
            return None;
        }
        std::fs::read_to_string(self.paths.progress_path(book_id))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
    }

    pub fn save_progress(&self, progress: &ReadingProgress) -> anyhow::Result<()> {
        validate_book_id(&progress.book_id)?;
        if let Some(parent) = self.paths.progress_path(&progress.book_id).parent() {
            std::fs::create_dir_all(parent)?;
        }
        atomic_write_json(&self.paths.progress_path(&progress.book_id), progress)?;
        Ok(())
    }

    pub fn add_book_with_format(
        &self,
        metadata: BookMetadata,
        file_path: &str,
        file_size: u64,
        format: &str,
    ) -> anyhow::Result<LibraryEntry> {
        self.add_book_with_id(&uuid::Uuid::new_v4().to_string(), metadata, file_path, file_size, format)
    }

    pub fn add_book_with_id(
        &self,
        id: &str,
        metadata: BookMetadata,
        file_path: &str,
        file_size: u64,
        format: &str,
    ) -> anyhow::Result<LibraryEntry> {
        let entry = LibraryEntry {
            id: id.to_string(),
            title: metadata.title.clone(),
            author: metadata.author.clone(),
            cover: None,
            file_path: file_path.to_string(),
            added_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            file_size,
            format: format.to_string(),
        };

        validate_book_id(id)?;
        let mut library = self.load_library_checked()?;
        library.push(entry.clone());
        self.save_library(&library)?;

        Ok(entry)
    }

    pub fn remove_book(&self, book_id: &str) -> anyhow::Result<()> {
        validate_book_id(book_id)?;
        let mut library = self.load_library_checked()?;
        let Some(entry) = library.iter().find(|e| e.id == book_id).cloned() else {
            return Ok(());
        };

        let format = if entry.format.is_empty() { "epub" } else { &entry.format };
        let managed_path = self.paths.book_path(book_id, format);
        if managed_path.exists() {
            std::fs::remove_file(&managed_path)?;
        }

        library.retain(|e| e.id != book_id);
        self.save_library(&library)?;

        // 写 tombstone，让 WebDAV 同步能把删除传播到其他设备。
        let tombstone_path = self.paths.tombstone_path(book_id);
        if let Some(parent) = tombstone_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tombstone = serde_json::json!({
            "book_id": book_id,
            "deleted_at": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            "device_id": "",
        });
        std::fs::write(&tombstone_path, serde_json::to_vec(&tombstone)?)?;

        // 只清理元数据（进度 + 封面），不动书籍原文件
        if let Err(e) = remove_file_if_exists(&self.paths.cover_path(book_id)) {
            eprintln!(
                "[Store::remove_book] failed to remove cover metadata for {}: {}",
                book_id, e
            );
        }
        if let Err(e) = remove_file_if_exists(&self.paths.progress_path(book_id)) {
            eprintln!(
                "[Store::remove_book] failed to remove progress metadata for {}: {}",
                book_id, e
            );
        }

        Ok(())
    }

    pub fn update_book_cover(&self, book_id: &str, cover_data_uri: &str) -> anyhow::Result<()> {
        validate_book_id(book_id)?;
        let mut library = self.load_library_checked()?;
        if let Some(entry) = library.iter_mut().find(|e| e.id == book_id) {
            entry.cover = Some(cover_data_uri.to_string());
            self.save_library(&library)?;
        }
        let cover_path = self.paths.cover_path(book_id);
        if let Some(parent) = cover_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // 写入封面文件
        if cover_data_uri.starts_with("data:image/") {
            if let Some(base64_start) = cover_data_uri.find("base64,") {
                let b64 = &cover_data_uri[base64_start + 7..];
                if let Ok(bytes) = base64::Engine::decode(
                    &base64::engine::general_purpose::STANDARD,
                    b64,
                ) {
                    let _ = std::fs::write(&cover_path, bytes);
                }
            }
        }
        Ok(())
    }
}

fn validate_book_id(id: &str) -> anyhow::Result<()> {
    if crate::storage::paths::is_safe_book_id(id) {
        Ok(())
    } else {
        Err(anyhow::anyhow!("书籍 ID 格式不合法"))
    }
}

fn load_json_with_backup<T>(path: &Path, label: &str) -> anyhow::Result<T>
where
    T: serde::de::DeserializeOwned + Default,
{
    if !path.exists() {
        return Ok(T::default());
    }
    let text = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("无法读取{label}: {e}"))?;
    match serde_json::from_str(&text) {
        Ok(value) => Ok(value),
        Err(primary) => {
            let backup = path.with_extension("json.bak");
            let backup_text = std::fs::read_to_string(&backup).map_err(|_| {
                anyhow::anyhow!("{label}文件已损坏，且没有可用备份: {primary}")
            })?;
            serde_json::from_str(&backup_text)
                .map_err(|e| anyhow::anyhow!("{label}文件和备份均无法解析: {primary}; {e}"))
        }
    }
}

pub(crate) fn atomic_write_json<T: serde::Serialize + ?Sized>(
    path: &Path,
    value: &T,
) -> anyhow::Result<()> {
    let json = serde_json::to_vec_pretty(value)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!("json.tmp-{}", uuid::Uuid::new_v4()));
    std::fs::write(&tmp, json)?;
    let backup = path.with_extension("json.bak");
    if path.exists() {
        std::fs::copy(path, &backup)?;
        std::fs::remove_file(path)?;
    }
    if let Err(error) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        if !path.exists() && backup.exists() {
            let _ = std::fs::rename(&backup, path);
        }
        return Err(error.into());
    }
    Ok(())
}

fn remove_file_if_exists(path: &std::path::Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> (Store, PathBuf) {
        let temp = std::env::temp_dir().join(format!("epubreader-store-test-{}", uuid::Uuid::new_v4()));
        (Store::new(Some(temp.clone())), temp)
    }

    #[test]
    fn rejects_book_ids_that_are_not_safe_path_components() {
        for id in ["../escape", "a/b", "a\\b", "", "book-1"] {
            assert_eq!(validate_book_id(id).is_ok(), id == "book-1");
        }
    }

    #[test]
    fn refuses_to_write_progress_for_an_unsafe_book_id() {
        let (store, temp) = test_store();
        let progress = ReadingProgress {
            book_id: "../escape".to_string(),
            chapter_index: 0,
            page_in_chapter: 0,
            total_pages_read: 0,
            last_read: 0,
        };

        assert!(store.save_progress(&progress).is_err());
        assert!(!temp.join("escape.json").exists());
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn restores_library_from_backup_when_primary_is_corrupt() {
        let (store, temp) = test_store();
        let expected = vec![LibraryEntry {
            id: "book-1".to_string(),
            title: "Book".to_string(),
            author: "Author".to_string(),
            cover: None,
            file_path: "book.epub".to_string(),
            added_at: 0,
            file_size: 0,
            format: "epub".to_string(),
        }];
        store.save_library(&expected).unwrap();
        store.save_library(&[]).unwrap();
        std::fs::write(&store.paths.library_file, b"not json").unwrap();

        assert_eq!(store.load_library_checked().unwrap()[0].id, "book-1");
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn reports_corrupt_library_when_no_backup_exists() {
        let (store, temp) = test_store();
        std::fs::write(&store.paths.library_file, b"not json").unwrap();

        assert!(store.load_library_checked().is_err());
        std::fs::remove_dir_all(temp).unwrap();
    }
}
