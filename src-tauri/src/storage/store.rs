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

/// 阅读状态：未读 / 在读 / 已读。已读只能由用户手动标记。
pub const READING_STATUS_UNREAD: &str = "unread";
pub const READING_STATUS_READING: &str = "reading";
pub const READING_STATUS_FINISHED: &str = "finished";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadingStatus {
    Unread,
    Reading,
    Finished,
}

impl ReadingStatus {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            READING_STATUS_UNREAD => Some(Self::Unread),
            READING_STATUS_READING => Some(Self::Reading),
            READING_STATUS_FINISHED => Some(Self::Finished),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unread => READING_STATUS_UNREAD,
            Self::Reading => READING_STATUS_READING,
            Self::Finished => READING_STATUS_FINISHED,
        }
    }
}

/// 阅读资料文件的结构版本。老文件缺少该字段时按 1 处理。
pub const READING_DATA_VERSION: u32 = 1;
/// 单本书保留的最近历史条数。
pub const HISTORY_LIMIT: usize = 200;
/// 书签备注长度上限。
pub const BOOKMARK_NOTE_LIMIT: usize = 500;
/// 书签正文摘要长度上限。
pub const BOOKMARK_SNIPPET_LIMIT: usize = 120;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bookmark {
    pub id: String,
    pub chapter_index: usize,
    /// 与前端 TextAnchor 同构：文本节点路径 + 字符偏移 + 摘要 + 章内比例。
    pub anchor: serde_json::Value,
    /// 正文摘要，用于展示和锚点失效时的回退定位。
    pub snippet: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: String,
    /// open / leave / jump：打开书、离开阅读、跳转前后。
    pub kind: String,
    pub chapter_index: usize,
    pub anchor: serde_json::Value,
    pub snippet: String,
    pub created_at: u64,
}

/// 每本书的阅读资料，独立于 progress/<id>.json 保存，便于整体同步和冲突选择。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ReadingData {
    pub version: u32,
    pub book_id: String,
    pub status: String,
    pub bookmarks: Vec<Bookmark>,
    pub history: Vec<HistoryEntry>,
    pub updated_at: u64,
}

impl Default for ReadingData {
    fn default() -> Self {
        Self {
            version: READING_DATA_VERSION,
            book_id: String::new(),
            status: READING_STATUS_UNREAD.to_string(),
            bookmarks: Vec::new(),
            history: Vec::new(),
            updated_at: 0,
        }
    }
}

impl ReadingData {
    pub fn for_book(book_id: &str) -> Self {
        Self {
            book_id: book_id.to_string(),
            ..Self::default()
        }
    }

    /// 同一章节、相同锚点路径视为同一书签，避免重复添加。
    pub fn find_bookmark_at(&self, chapter_index: usize, anchor: &serde_json::Value) -> Option<&Bookmark> {
        self.bookmarks.iter().find(|b| {
            b.chapter_index == chapter_index && same_anchor(&b.anchor, anchor)
        })
    }

    /// 追加历史并去重：与最新一条位置相同则只替换，不新增条目。
    pub fn push_history(&mut self, entry: HistoryEntry) {
        if let Some(last) = self.history.first() {
            if last.chapter_index == entry.chapter_index && same_anchor(&last.anchor, &entry.anchor) {
                self.history[0] = entry;
                return;
            }
        }
        self.history.insert(0, entry);
        if self.history.len() > HISTORY_LIMIT {
            self.history.truncate(HISTORY_LIMIT);
        }
    }
}

/// 只比较能代表「同一处正文位置」的部分：文本节点路径 + 字符偏移。
/// 两者都参与比较，否则同一个文本节点里的不同位置会被误判成同一处。
fn same_anchor(left: &serde_json::Value, right: &serde_json::Value) -> bool {
    let left_path = left.get("path").and_then(|v| v.as_array());
    let right_path = right.get("path").and_then(|v| v.as_array());
    let left_offset = left.get("offset").and_then(|v| v.as_u64());
    let right_offset = right.get("offset").and_then(|v| v.as_u64());
    left_path == right_path && left_offset == right_offset
}

fn compare_anchor(left: &serde_json::Value, right: &serde_json::Value) -> std::cmp::Ordering {
    let path = |value: &serde_json::Value| -> Vec<u64> {
        value.get("path").and_then(|v| v.as_array())
            .map(|items| items.iter().filter_map(|v| v.as_u64()).collect())
            .unwrap_or_default()
    };
    path(left).cmp(&path(right))
        .then_with(|| left.get("offset").and_then(|v| v.as_u64()).unwrap_or(0)
            .cmp(&right.get("offset").and_then(|v| v.as_u64()).unwrap_or(0)))
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn truncate_snippet(value: String) -> String {
    truncate_chars(value, BOOKMARK_SNIPPET_LIMIT)
}

fn truncate_note(value: String) -> String {
    truncate_chars(value, BOOKMARK_NOTE_LIMIT)
}

fn truncate_chars(value: String, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value;
    }
    value.chars().take(limit).collect()
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

    /// 读取单本书的阅读资料。文件缺失时返回按进度推导的初始状态，不落盘。
    pub fn load_reading_data(&self, book_id: &str) -> anyhow::Result<ReadingData> {
        validate_book_id(book_id)?;
        let path = self.paths.reading_data_path(book_id);
        if path.exists() {
            match load_json_with_backup::<ReadingData>(&path, "阅读资料") {
                Ok(mut data) => {
                    data.book_id = book_id.to_string();
                    if data.version == 0 {
                        data.version = READING_DATA_VERSION;
                    }
                    if ReadingStatus::parse(&data.status).is_none() {
                        data.status = READING_STATUS_UNREAD.to_string();
                    }
                    return Ok(data);
                }
                Err(error) => return Err(error),
            }
        }
        Ok(self.derived_reading_data(book_id))
    }

    /// 列表读取用的汇总：状态 + 书签数 + 历史数 + 更新时间。
    pub fn load_all_reading_summaries(&self) -> Vec<ReadingData> {
        let mut items: Vec<ReadingData> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.paths.reading_data_dir) {
            for entry in entries.flatten() {
                if entry.path().extension().map(|e| e == "json").unwrap_or(false) {
                    let path = entry.path();
                    let Some(stem) = path.file_stem().and_then(|s| s.to_str().map(str::to_string)) else {
                        continue;
                    };
                    if validate_book_id(&stem).is_err() {
                        continue;
                    }
                    if let Ok(data) = self.load_reading_data(&stem) {
                        items.push(data);
                    }
                }
            }
        }
        items
    }

    /// 阅读资料文件不存在时，按旧进度推导未读 / 在读。
    /// 有进度记录即为「在读」；没有进度记录即为「未读」。
    fn derived_reading_data(&self, book_id: &str) -> ReadingData {
        let status = if self.load_progress(book_id).is_some() {
            READING_STATUS_READING
        } else {
            READING_STATUS_UNREAD
        };
        ReadingData {
            book_id: book_id.to_string(),
            status: status.to_string(),
            ..ReadingData::default()
        }
    }

    pub fn save_reading_data(&self, data: &ReadingData) -> anyhow::Result<()> {
        validate_book_id(&data.book_id)?;
        if let Some(parent) = self.paths.reading_data_path(&data.book_id).parent() {
            std::fs::create_dir_all(parent)?;
        }
        atomic_write_json(&self.paths.reading_data_path(&data.book_id), data)?;
        Ok(())
    }

    pub fn set_reading_status(&self, book_id: &str, status: &str) -> anyhow::Result<ReadingData> {
        let Some(parsed) = ReadingStatus::parse(status) else {
            return Err(anyhow::anyhow!("阅读状态不合法: {}", status));
        };
        let mut data = self.load_reading_data(book_id)?;
        data.status = parsed.as_str().to_string();
        data.version = READING_DATA_VERSION;
        data.updated_at = now_secs();
        self.save_reading_data(&data)?;
        Ok(data)
    }

    /// 在持有 Store 锁时执行状态推进；已读状态不能被自动打开操作降级。
    pub fn mark_reading_started(&self, book_id: &str) -> anyhow::Result<ReadingData> {
        let data = self.load_reading_data(book_id)?;
        if data.status != READING_STATUS_UNREAD {
            return Ok(data);
        }
        self.set_reading_status(book_id, READING_STATUS_READING)
    }

    /// 添加书签。同一位置已有书签时返回该书签（不新增）。
    pub fn add_bookmark(
        &self,
        book_id: &str,
        chapter_index: usize,
        anchor: serde_json::Value,
        snippet: String,
        note: Option<String>,
    ) -> anyhow::Result<(ReadingData, bool)> {
        let mut data = self.load_reading_data(book_id)?;
        if let Some(existing) = data.find_bookmark_at(chapter_index, &anchor) {
            let id = existing.id.clone();
            eprintln!("[Store::add_bookmark] existing bookmark at same position: {}", id);
            return Ok((data, false));
        }
        let bookmark = Bookmark {
            id: uuid::Uuid::new_v4().to_string(),
            chapter_index,
            anchor,
            snippet: truncate_snippet(snippet),
            note: note.map(|n| truncate_note(n)),
            created_at: now_secs(),
        };
        // 正文顺序：章节、文本节点路径、字符偏移。
        data.bookmarks.push(bookmark);
        data.bookmarks.sort_by(|a, b| {
            a.chapter_index
                .cmp(&b.chapter_index)
                .then_with(|| compare_anchor(&a.anchor, &b.anchor))
                .then(a.created_at.cmp(&b.created_at))
        });
        data.version = READING_DATA_VERSION;
        data.updated_at = now_secs();
        self.save_reading_data(&data)?;
        Ok((data, true))
    }

    pub fn update_bookmark_note(
        &self,
        book_id: &str,
        bookmark_id: &str,
        note: Option<String>,
    ) -> anyhow::Result<ReadingData> {
        let mut data = self.load_reading_data(book_id)?;
        let Some(bookmark) = data.bookmarks.iter_mut().find(|b| b.id == bookmark_id) else {
            return Err(anyhow::anyhow!("书签不存在"));
        };
        bookmark.note = note.map(|n| truncate_note(n));
        data.version = READING_DATA_VERSION;
        data.updated_at = now_secs();
        self.save_reading_data(&data)?;
        Ok(data)
    }

    pub fn remove_bookmark(&self, book_id: &str, bookmark_id: &str) -> anyhow::Result<ReadingData> {
        let mut data = self.load_reading_data(book_id)?;
        let before = data.bookmarks.len();
        data.bookmarks.retain(|b| b.id != bookmark_id);
        if data.bookmarks.len() == before {
            return Err(anyhow::anyhow!("书签不存在"));
        }
        data.version = READING_DATA_VERSION;
        data.updated_at = now_secs();
        self.save_reading_data(&data)?;
        Ok(data)
    }

    /// 追加一条历史。位置与最新一条相同时只更新时间。
    pub fn append_history(
        &self,
        book_id: &str,
        kind: &str,
        chapter_index: usize,
        anchor: serde_json::Value,
        snippet: String,
    ) -> anyhow::Result<ReadingData> {
        let mut data = self.load_reading_data(book_id)?;
        data.push_history(HistoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            kind: kind.to_string(),
            chapter_index,
            anchor,
            snippet: truncate_snippet(snippet),
            created_at: now_secs(),
        });
        data.version = READING_DATA_VERSION;
        data.updated_at = now_secs();
        self.save_reading_data(&data)?;
        Ok(data)
    }

    pub fn clear_history(&self, book_id: &str) -> anyhow::Result<ReadingData> {
        let mut data = self.load_reading_data(book_id)?;
        data.history.clear();
        data.version = READING_DATA_VERSION;
        data.updated_at = now_secs();
        self.save_reading_data(&data)?;
        Ok(data)
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
        if let Err(e) = remove_file_if_exists(&self.paths.reading_data_path(book_id)) {
            eprintln!(
                "[Store::remove_book] failed to remove reading data for {}: {}",
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

    fn anchor_at(path: &[u64], offset: u64) -> serde_json::Value {
        serde_json::json!({
            "path": path,
            "offset": offset,
            "snippet": "x",
            "ratio": 0.5
        })
    }

    #[test]
    fn reading_status_round_trips_and_rejects_unknown_values() {
        let (store, temp) = test_store();

        store
            .set_reading_status("book-1", READING_STATUS_FINISHED)
            .unwrap();
        let data = store.load_reading_data("book-1").unwrap();
        assert_eq!(data.status, READING_STATUS_FINISHED);

        assert!(store.set_reading_status("book-1", "bogus").is_err());
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn reading_status_is_derived_from_progress_when_no_file_exists() {
        let (store, temp) = test_store();
        assert_eq!(
            store.load_reading_data("book-1").unwrap().status,
            READING_STATUS_UNREAD
        );

        store
            .save_progress(&ReadingProgress {
                book_id: "book-1".to_string(),
                chapter_index: 0,
                page_in_chapter: 0,
                total_pages_read: 0,
                last_read: 1,
                anchor: None,
                mode: None,
            })
            .unwrap();
        assert_eq!(
            store.load_reading_data("book-1").unwrap().status,
            READING_STATUS_READING
        );
        // 推导状态不落盘
        assert!(!store.paths.reading_data_path("book-1").exists());
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn bookmark_at_the_same_position_is_not_duplicated() {
        let (store, temp) = test_store();
        let anchor = anchor_at(&[1, 2], 5);

        let (_, created) = store
            .add_bookmark("book-1", 0, anchor.clone(), "片段".to_string(), None)
            .unwrap();
        assert!(created);

        let (data, created_again) = store
            .add_bookmark("book-1", 0, anchor.clone(), "片段".to_string(), None)
            .unwrap();
        assert!(!created_again);
        assert_eq!(data.bookmarks.len(), 1);

        // 不同章节视为不同书签
        let (_, created_other) = store
            .add_bookmark("book-1", 1, anchor, "片段".to_string(), None)
            .unwrap();
        assert!(created_other);
        assert_eq!(store.load_reading_data("book-1").unwrap().bookmarks.len(), 2);
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn bookmark_note_and_snippet_are_truncated_to_limits() {
        let (store, temp) = test_store();
        let long_note = "笔".repeat(BOOKMARK_NOTE_LIMIT + 50);
        let long_snippet = "词".repeat(BOOKMARK_SNIPPET_LIMIT + 50);

        store
            .add_bookmark(
                "book-1",
                0,
                anchor_at(&[0], 0),
                long_snippet,
                Some(long_note),
            )
            .unwrap();

        let data = store.load_reading_data("book-1").unwrap();
        let bookmark = &data.bookmarks[0];
        assert_eq!(bookmark.note.as_ref().unwrap().chars().count(), BOOKMARK_NOTE_LIMIT);
        assert_eq!(bookmark.snippet.chars().count(), BOOKMARK_SNIPPET_LIMIT);
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn bookmarks_are_sorted_by_reading_order() {
        let (store, temp) = test_store();
        store
            .add_bookmark("book-1", 3, anchor_at(&[0], 0), "c".to_string(), None)
            .unwrap();
        store
            .add_bookmark("book-1", 1, anchor_at(&[0], 1), "a".to_string(), None)
            .unwrap();
        store
            .add_bookmark("book-1", 2, anchor_at(&[0], 2), "b".to_string(), None)
            .unwrap();

        let data = store.load_reading_data("book-1").unwrap();
        let order: Vec<usize> = data.bookmarks.iter().map(|b| b.chapter_index).collect();
        assert_eq!(order, vec![1, 2, 3]);
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn consecutive_history_at_the_same_position_is_merged() {
        let (store, temp) = test_store();
        let anchor = anchor_at(&[0], 3);

        store
            .append_history("book-1", "open", 0, anchor.clone(), "a".to_string())
            .unwrap();
        store
            .append_history("book-1", "jump", 0, anchor.clone(), "a".to_string())
            .unwrap();

        let data = store.load_reading_data("book-1").unwrap();
        assert_eq!(data.history.len(), 1);
        assert_eq!(data.history[0].kind, "jump");

        store
            .append_history("book-1", "leave", 1, anchor_at(&[0], 9), "b".to_string())
            .unwrap();
        assert_eq!(store.load_reading_data("book-1").unwrap().history.len(), 2);
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn history_is_capped_at_the_limit() {
        let (store, temp) = test_store();
        for index in 0..(HISTORY_LIMIT + 25) {
            store
                .append_history(
                    "book-1",
                    "jump",
                    0,
                    anchor_at(&[0], index as u64),
                    "x".to_string(),
                )
                .unwrap();
        }
        let data = store.load_reading_data("book-1").unwrap();
        assert_eq!(data.history.len(), HISTORY_LIMIT);
        // 最新的在最前面
        assert_eq!(
            data.history[0].anchor.get("offset").unwrap().as_u64().unwrap(),
            (HISTORY_LIMIT + 24) as u64
        );
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn clear_history_keeps_bookmarks_and_status() {
        let (store, temp) = test_store();
        store
            .set_reading_status("book-1", READING_STATUS_FINISHED)
            .unwrap();
        store
            .add_bookmark("book-1", 0, anchor_at(&[0], 0), "x".to_string(), None)
            .unwrap();
        store
            .append_history("book-1", "open", 0, anchor_at(&[0], 0), "x".to_string())
            .unwrap();

        let data = store.clear_history("book-1").unwrap();
        assert!(data.history.is_empty());
        assert_eq!(data.bookmarks.len(), 1);
        assert_eq!(data.status, READING_STATUS_FINISHED);
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn removing_a_book_removes_its_reading_data() {
        let (store, temp) = test_store();
        let metadata = crate::epub::parser::BookMetadata {
            title: "Book".to_string(),
            author: String::new(),
            language: String::new(),
            publisher: String::new(),
            description: String::new(),
            cover_href: None,
        };
        store
            .add_book_with_id("book-1", metadata, "books/book-1.epub", 0, "epub")
            .unwrap();
        store
            .add_bookmark("book-1", 0, anchor_at(&[0], 0), "x".to_string(), None)
            .unwrap();
        assert!(store.paths.reading_data_path("book-1").exists());

        store.remove_book("book-1").unwrap();
        assert!(!store.paths.reading_data_path("book-1").exists());
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn reading_data_refuses_unsafe_book_ids() {
        let (store, temp) = test_store();
        assert!(store.save_reading_data(&ReadingData::for_book("../escape")).is_err());
        assert!(store.load_reading_data("../escape").is_err());
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn auto_start_never_downgrades_finished_and_promotes_unread() {
        let (store, temp) = test_store();
        assert_eq!(store.mark_reading_started("book-1").unwrap().status, READING_STATUS_READING);
        store.set_reading_status("book-1", READING_STATUS_FINISHED).unwrap();
        let before = std::fs::read(store.paths.reading_data_path("book-1")).unwrap();
        assert_eq!(store.mark_reading_started("book-1").unwrap().status, READING_STATUS_FINISHED);
        assert_eq!(before, std::fs::read(store.paths.reading_data_path("book-1")).unwrap());
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn corrupt_reading_data_is_never_overwritten_by_mutation() {
        let (store, temp) = test_store();
        let path = store.paths.reading_data_path("book-1");
        std::fs::write(&path, b"broken data").unwrap();
        std::fs::write(path.with_extension("json.bak"), b"broken backup").unwrap();
        assert!(store.mark_reading_started("book-1").is_err());
        assert!(store.add_bookmark("book-1", 0, anchor_at(&[0], 0), "x".into(), None).is_err());
        assert!(store.append_history("book-1", "open", 0, anchor_at(&[0], 0), "x".into()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"broken data");
        assert_eq!(std::fs::read(path.with_extension("json.bak")).unwrap(), b"broken backup");
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn same_chapter_bookmarks_follow_anchor_order_not_creation_order() {
        let (store, temp) = test_store();
        store.add_bookmark("book-1", 0, anchor_at(&[2], 10), "last".into(), None).unwrap();
        store.add_bookmark("book-1", 0, anchor_at(&[1], 20), "first".into(), None).unwrap();
        store.add_bookmark("book-1", 0, anchor_at(&[2], 1), "middle".into(), None).unwrap();
        let snippets: Vec<String> = store.load_reading_data("book-1").unwrap().bookmarks.into_iter().map(|b| b.snippet).collect();
        assert_eq!(snippets, vec!["first", "middle", "last"]);
        std::fs::remove_dir_all(temp).unwrap();
    }
}
