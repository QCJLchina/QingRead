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
    /// 上次阅读时间，Unix 秒（与库内其他时间戳一致）。
    pub last_read: u64,
    /// 稳定的正文锚点：清洗后正文里的文本节点路径 + 字符偏移，附带用于重定位的
    /// 文本片段和章节内比例。旧的进度文件没有这个字段，读取时按页码尽力恢复。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<serde_json::Value>,
    /// 上次使用的阅读模式：`paged` 或 `scroll`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
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
    /// 阅读模式：`paged`（分页）或 `scroll`（滚动）。
    /// `None` 表示用户从未选择过：升级上来的老安装保持滚动习惯，
    /// 全新安装由 `Store::new` 写入 `paged`。
    #[serde(default)]
    pub reading_mode: Option<String>,
    /// 正文栏宽度（逻辑像素）。`None` 或 0 表示跟随窗口宽度。
    #[serde(default)]
    pub content_width: Option<f32>,
    /// 正文内边距（逻辑像素）。
    #[serde(default)]
    pub content_padding: Option<f32>,
}

/// 本机窗口与低干扰偏好。
///
/// 刻意存放在独立的 window.json，而不是 settings.json：
/// 窗口位置/尺寸属于「这台电脑」的状态，换机器后套用旧坐标会跑到屏幕外；
/// 而且它不参与 WebDAV 同步（同步只覆盖 books/、covers/、progress/、tombstones/）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowSettings {
    /// 上次使用的布局预设：standard / slim / strip / mini / custom
    pub layout: String,
    /// 窗口客户区宽高，逻辑像素
    pub width: f64,
    pub height: f64,
    /// 窗口左上角坐标，逻辑像素。仅在能映射到显示器时恢复。
    pub x: Option<f64>,
    pub y: Option<f64>,
    /// 是否锁定当前长宽比例
    pub lock_ratio: bool,
    pub always_on_top: bool,
    /// 低干扰模式：中性窗口标题、隐藏装饰背景、收起次要操作
    pub low_distraction: bool,
    /// 全局隐藏/恢复快捷键，形如 `Ctrl+Alt+H`
    pub hide_hotkey: String,
    /// 鼠标移出后自动收起工具栏
    pub toolbar_auto_hide: bool,
    /// 窗口失去焦点时显示中性遮挡页
    pub blur_curtain: bool,
}

impl Default for WindowSettings {
    fn default() -> Self {
        Self {
            layout: "standard".to_string(),
            width: 940.0,
            height: 610.0,
            x: None,
            y: None,
            lock_ratio: false,
            always_on_top: false,
            low_distraction: false,
            hide_hotkey: "Ctrl+Alt+H".to_string(),
            toolbar_auto_hide: false,
            blur_curtain: false,
        }
    }
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
            reading_mode: None,
            content_width: None,
            content_padding: None,
        }
    }
}

impl AppSettings {
    /// 全新安装的默认值：默认分页阅读。
    ///
    /// 只有数据目录里还不存在 settings.json 时才会用它 —— 老用户升级后文件已存在，
    /// 反序列化得到 `reading_mode: None`，前端据此继续使用滚动阅读，保持原有习惯。
    pub fn fresh_install_default() -> Self {
        Self {
            reading_mode: Some("paged".to_string()),
            ..Self::default()
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
        // 文件不存在 = 全新安装：返回「默认分页」的初始值，但不落盘。
        // 刻意不在这里创建 settings.json —— 同步逻辑把「本地多出一个 settings.json」
        // 当成一次本地修改，凭空造出与远端的伪冲突；老安装文件已存在，
        // 反序列化得到 reading_mode: None，前端据此继续保持滚动阅读习惯。
        if !self.paths.settings_file.exists() {
            return AppSettings::fresh_install_default();
        }
        load_json_with_backup(&self.paths.settings_file, "设置").unwrap_or_default()
    }

    pub fn save_settings(&self, settings: &AppSettings) -> anyhow::Result<()> {
        if let Some(parent) = self.paths.settings_file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        atomic_write_json(&self.paths.settings_file, settings)?;
        Ok(())
    }

    pub fn load_window_settings(&self) -> WindowSettings {
        load_json_with_backup(&self.paths.window_file, "窗口设置").unwrap_or_default()
    }

    pub fn save_window_settings(&self, settings: &WindowSettings) -> anyhow::Result<()> {
        if let Some(parent) = self.paths.window_file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        atomic_write_json(&self.paths.window_file, settings)?;
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


    /// 扫描进度目录，返回所有书的阅读进度。
    /// 用于书架上的「继续阅读」和进度显示。单本解析失败不影响其他书。
    pub fn load_all_progress(&self) -> Vec<ReadingProgress> {
        let Ok(entries) = std::fs::read_dir(&self.paths.progress_dir) else {
            return Vec::new();
        };
        let mut items: Vec<ReadingProgress> = entries
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().extension().map(|e| e == "json").unwrap_or(false))
            .filter_map(|entry| std::fs::read_to_string(entry.path()).ok())
            .filter_map(|text| serde_json::from_str::<ReadingProgress>(&text).ok())
            .collect();
        items.sort_by(|a, b| b.last_read.cmp(&a.last_read));
        items
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
        let temp = std::env::temp_dir().join(format!("qingread-store-test-{}", uuid::Uuid::new_v4()));
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
            anchor: None,
            mode: None,
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
