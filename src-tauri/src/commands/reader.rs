use crate::epub::parser::EpubParser;
use crate::epub::sanitizer::sanitize_html;
use crate::epub::txt_parser::TxtParser;
use crate::state::AppState;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};

/// 把所有 load_chapter 流程日志写到 %USERPROFILE%\epubreader-debug.log，
/// 这样用户用安装包跑时也能拿到诊断信息
fn debug_log(msg: &str) {
    if !debug_logging_enabled() {
        return;
    }
    if let Some(home) = std::env::var_os("USERPROFILE") {
        let path = std::path::PathBuf::from(home).join("epubreader-debug.log");
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
            let _ = writeln!(f, "{}", msg);
        }
    }
}

fn debug_logging_enabled() -> bool {
    match std::env::var("EPUBREADER_DEBUG") {
        Ok(value) => !value.is_empty() && value != "0" && value.to_ascii_lowercase() != "false",
        Err(_) => false,
    }
}

/// 已 sanitize 的章节缓存：key = (book_id, chapter_index)，value = ChapterData
/// 同一本书切回之前读过的章节时直接返回，不再重新打开 zip / sanitize。
/// 简单 LRU：超出容量时淘汰最久未访问的条目，避免内存无限增长。
const CHAPTER_CACHE_CAPACITY: usize = 64;

struct ChapterCache {
    map: HashMap<(String, usize), ChapterData>,
    order: VecDeque<(String, usize)>,
}

static SANITIZED_CHAPTER_CACHE: Lazy<Mutex<ChapterCache>> = Lazy::new(|| {
    Mutex::new(ChapterCache {
        map: HashMap::new(),
        order: VecDeque::new(),
    })
});

fn cache_get(book_id: &str, idx: usize) -> Option<ChapterData> {
    let mut cache = SANITIZED_CHAPTER_CACHE.lock().unwrap();
    let key = (book_id.to_string(), idx);
    if let Some(v) = cache.map.get(&key).cloned() {
        if let Some(pos) = cache.order.iter().position(|k| k == &key) {
            cache.order.remove(pos);
        }
        cache.order.push_back(key);
        Some(v)
    } else {
        None
    }
}

fn cache_put(book_id: String, idx: usize, data: ChapterData) {
    let mut cache = SANITIZED_CHAPTER_CACHE.lock().unwrap();
    let key = (book_id, idx);
    if cache.map.contains_key(&key) {
        if let Some(pos) = cache.order.iter().position(|k| k == &key) {
            cache.order.remove(pos);
        }
    }
    cache.map.insert(key.clone(), data);
    cache.order.push_back(key);
    while cache.map.len() > CHAPTER_CACHE_CAPACITY {
        if let Some(old) = cache.order.pop_front() {
            cache.map.remove(&old);
        } else {
            break;
        }
    }
}

pub fn invalidate_book(book_id: &str) {
    let mut cache = SANITIZED_CHAPTER_CACHE.lock().unwrap();
    cache.map.retain(|(id, _), _| id != book_id);
    cache.order.retain(|(id, _)| id != book_id);
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChapterData {
    pub index: usize,
    pub title: String,
    pub content: String,
}

impl Clone for ChapterData {
    fn clone(&self) -> Self {
        Self {
            index: self.index,
            title: self.title.clone(),
            content: self.content.clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TocEntry {
    pub title: String,
    pub href: String,
    pub level: usize,
    pub chapter_index: Option<usize>,
}

/// 解析书架条目，拿到磁盘上的实际文件路径
/// 优先用数据目录 books/{id}.{ext}（导入时已复制过去），
/// 找不到时回退到 entry.file_path（兼容老数据/外部引用）。
fn resolve_book_path(
    store: &crate::storage::store::Store,
    book_id: &str,
) -> Result<Option<(PathBuf, String)>, String> {
    if !crate::storage::paths::is_safe_book_id(book_id) {
        return Ok(None);
    }
    let library = store.load_library_checked().map_err(|e| e.to_string())?;
    let Some(entry) = library.iter().find(|e| e.id == book_id) else {
        return Ok(None);
    };
    let format = if entry.format.is_empty() { "epub" } else { &entry.format };

    let managed = store.paths.book_path(book_id, format);
    if managed.exists() {
        return Ok(Some((managed, format.to_string())));
    }

    let original = PathBuf::from(&entry.file_path);
    if original.exists() {
        return Ok(Some((original, format.to_string())));
    }
    Ok(None)
}

/// 把 TOC 条目的 href 映射回 spine 序号，避免目录顺序与正文顺序不一致时跳错章节。
fn epub_toc_chapter_index(
    book: &crate::epub::parser::BookIndex,
    href: &str,
) -> Option<usize> {
    let clean = href.split('#').next().unwrap_or(href);
    let clean_basename = Path::new(clean).file_name().and_then(|n| n.to_str());
    let mut basename_match = None;

    for (index, spine_id) in book.spine.iter().enumerate() {
        let Some(item) = book.manifest.get(spine_id) else { continue; };
        if clean == item.href || clean.ends_with(&item.href) || item.href.ends_with(clean) {
            return Some(index);
        }
        let item_basename = Path::new(&item.href).file_name().and_then(|n| n.to_str());
        if clean_basename.is_some()
            && clean_basename == item_basename
            && basename_match.is_none()
        {
            basename_match = Some(index);
        }
    }

    basename_match
}

#[tauri::command]
pub async fn open_reader(book_id: String, state: State<'_, AppState>) -> Result<usize, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let (path, format) = resolve_book_path(&store, &book_id)?
        .ok_or_else(|| "Book file not found".to_string())?;

    if format == "txt" {
        let index = TxtParser::quick_scan(&path)
            .map_err(|e| format!("Failed to scan TXT: {}", e))?;
        Ok(index.chapters.len())
    } else {
        EpubParser::chapter_count(&path)
            .map_err(|e| format!("Failed to get chapter count: {}", e))
    }
}

#[tauri::command]
pub async fn load_chapter(
    book_id: String,
    chapter_index: usize,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ChapterData, String> {
    debug_log(&format!("[load_chapter] start: book_id={}, chapter_index={}", book_id, chapter_index));

    // 缓存命中：直接返回，不再解析/sanitize
    if let Some(hit) = cache_get(&book_id, chapter_index) {
        debug_log(&format!("[load_chapter] cache hit for ({}, {})", book_id, chapter_index));
        let _ = app.emit("load-progress", serde_json::json!({
            "stage": "已缓存",
            "current": 5,
            "total": 5,
        }));
        return Ok(hit);
    }
    debug_log("[load_chapter] cache miss, will parse");

    let (path, format) = {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        resolve_book_path(&store, &book_id)?
            .ok_or_else(|| "Book file not found".to_string())?
    };

    debug_log(&format!("[load_chapter] resolved, book_id={}, chapter_index={}, format={}", book_id, chapter_index, format));

    let _ = app.emit("load-progress", serde_json::json!({
        "stage": "开始",
        "current": 0,
        "total": 5,
        "message": "正在加载章节...",
    }));
    let _ = app.emit("load-progress", serde_json::json!({
        "stage": "解析文件",
        "current": 1,
        "total": 5,
    }));

    // 注意：Tauri 2 中 #[tauri::command] async fn 实际上跑在 webview 主线程的 tokio runtime 上。
    // 同步阻塞会卡 webview 事件循环，导致前端进度 listener 不跑、loading 一直显示。
    // 用 spawn_blocking 把它挪到独立线程，主线程立即 await 让出。
    let app_for_progress = app.clone();
    let format_owned = format.clone();
    let book_id_for_cb = book_id.clone();
    debug_log(&format!("[load_chapter] spawning parse task, format={}", format_owned));
    let parse_result: Result<(String, String, usize), String> = tokio::task::spawn_blocking(move || {
        let t1 = std::time::Instant::now();
        debug_log(&format!("[load_chapter] parse task started, book={}", book_id_for_cb));
        let emit_progress = |cur: usize, _total: usize, msg: &str| {
            // 在 spawn_blocking 线程里直接调 emit 是危险的：emit 内部要拿
            // webview 的锁，可能和 webview 主线程的 emit/事件派发死锁。
            // 改成 fire-and-forget 的 async task。
            let app = app_for_progress.clone();
            let stage = msg.to_string();
            tauri::async_runtime::spawn(async move {
                let _ = app.emit("load-progress", serde_json::json!({
                    "stage": stage,
                    "current": 2 + cur,
                    "total": 5,
                }));
            });
        };
        let res = if format_owned == "txt" {
            match TxtParser::load_chapter(&path, chapter_index) {
                Ok(ch) => {
                    let dur = t1.elapsed();
                    debug_log(&format!("[load_chapter] TXT done in {:?}", dur));
                    emit_progress(2, 5, "解析完成");
                    Ok((ch.title, ch.content, ch.index))
                }
                Err(e) => {
                    debug_log(&format!("[load_chapter] TXT FAILED: {}", e));
                    Err(format!("Failed to load chapter: {}", e))
                }
            }
        } else {
            let book_id_dbg = book_id_for_cb.clone();
            let chapter_dbg = chapter_index;
            match EpubParser::get_chapter_with_progress(&path, chapter_index, |cur, _total, msg| {
                debug_log(&format!("[load_chapter] ep cb: book={} ch={} cur={} msg={}", book_id_dbg, chapter_dbg, cur, msg));
                emit_progress(cur, 5, msg);
            }) {
                Ok(ch) => {
                    let dur = t1.elapsed();
                    debug_log(&format!("[load_chapter] EPUB done in {:?}, content size={}", dur, ch.content.len()));
                    Ok((ch.title, ch.content, ch.index))
                }
                Err(e) => {
                    debug_log(&format!("[load_chapter] EPUB FAILED: {}", e));
                    Err(format!("Failed to load chapter: {}", e))
                }
            }
        };
        debug_log("[load_chapter] parse task finished");
        res
    })
    .await
    .map_err(|e| {
        debug_log(&format!("[load_chapter] JoinError: {}", e));
        format!("Join error: {}", e)
    })?;

    let (chapter_title, chapter_content, chapter_idx) = parse_result?;
    debug_log(&format!("[load_chapter] got content, size={}, starting sanitize", chapter_content.len()));

    let _ = app.emit("load-progress", serde_json::json!({
        "stage": "清洗 HTML",
        "current": 4,
        "total": 5,
    }));

    let t3 = std::time::Instant::now();
    let format_for_sanitize = format.clone();
    let chapter_content_for_sanitize = chapter_content.clone();
    let sanitized: String = tokio::task::spawn_blocking(move || {
        if format_for_sanitize == "txt" {
            chapter_content_for_sanitize
        } else {
            sanitize_html(&chapter_content_for_sanitize)
        }
    })
    .await
    .map_err(|e| {
        debug_log(&format!("[load_chapter] Sanitize JoinError: {}", e));
        format!("Sanitize join error: {}", e)
    })?;
    debug_log(&format!("[load_chapter] sanitize done in {:?}, final size={}", t3.elapsed(), sanitized.len()));

    let _ = app.emit("load-progress", serde_json::json!({
        "stage": "完成",
        "current": 5,
        "total": 5,
    }));

    let data = ChapterData {
        index: chapter_idx,
        title: chapter_title,
        content: sanitized,
    };

    cache_put(book_id, chapter_index, data.clone());
    debug_log(&format!("[load_chapter] cached and returning, size={}", data.content.len()));

    Ok(data)
}

#[tauri::command]
pub async fn get_toc(book_id: String, state: State<'_, AppState>) -> Result<Vec<TocEntry>, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let (path, format) = resolve_book_path(&store, &book_id)?
        .ok_or_else(|| "Book file not found".to_string())?;

    if format == "txt" {
        let index = TxtParser::quick_scan(&path)
            .map_err(|e| format!("Failed to scan TXT: {}", e))?;
        Ok(index.chapters.iter().map(|c| TocEntry {
            title: c.title.clone(),
            href: format!("chapter_{}.xhtml", c.index),
            level: 0,
            chapter_index: Some(c.index),
        }).collect())
    } else {
        let book_index = EpubParser::load_index(&path)
            .map_err(|e| e.to_string())?;
        Ok(book_index.toc.iter().map(|t| TocEntry {
            title: t.title.clone(),
            href: t.href.clone(),
            level: t.level,
            chapter_index: epub_toc_chapter_index(&book_index, &t.href),
        }).collect())
    }
}

#[tauri::command]
pub async fn get_chapter_offsets(
    book_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<usize>, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let (path, format) = resolve_book_path(&store, &book_id)?
        .ok_or_else(|| "Book file not found".to_string())?;

    const CHARS_PER_PAGE: usize = 2000;

    // EPUB: 从 zip entry size 估算（不读内容，<1ms）
    // TXT:  从 quick_scan 缓存读取（<1ms）
    let char_counts: Vec<usize> = if format == "txt" {
        let index = TxtParser::quick_scan(&path)
            .map_err(|e| format!("Failed to scan TXT: {}", e))?;
        index.chapters.iter().map(|c| c.char_count).collect()
    } else {
        EpubParser::chapter_text_lengths(&path)
            .map_err(|e| format!("Failed to compute chapter lengths: {}", e))?
    };

    let pages_per_chapter: Vec<usize> = char_counts
        .iter()
        .map(|&len| std::cmp::max(1, (len + CHARS_PER_PAGE - 1) / CHARS_PER_PAGE))
        .collect();

    let mut offsets = Vec::with_capacity(pages_per_chapter.len() + 1);
    offsets.push(0);
    let mut total = 0;
    for &pages in &pages_per_chapter {
        total += pages;
        offsets.push(total);
    }
    Ok(offsets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::epub::parser::{BookMetadata, ManifestItem};

    fn sample_book() -> crate::epub::parser::BookIndex {
        crate::epub::parser::BookIndex {
            metadata: BookMetadata {
                title: "Test".to_string(),
                author: String::new(),
                language: String::new(),
                publisher: String::new(),
                description: String::new(),
                cover_href: None,
            },
            spine: vec!["c1".to_string(), "c2".to_string()],
            manifest: HashMap::from([
                (
                    "c1".to_string(),
                    ManifestItem {
                        id: "c1".to_string(),
                        href: "Text/ch1.xhtml".to_string(),
                        media_type: "application/xhtml+xml".to_string(),
                    },
                ),
                (
                    "c2".to_string(),
                    ManifestItem {
                        id: "c2".to_string(),
                        href: "Text/ch2.xhtml".to_string(),
                        media_type: "application/xhtml+xml".to_string(),
                    },
                ),
            ]),
            toc: Vec::new(),
            opf_dir: "OEBPS".to_string(),
            cover_data_uri: None,
            estimated_chars: Vec::new(),
        }
    }

    #[test]
    fn maps_toc_hrefs_to_spine_indexes() {
        let book = sample_book();
        assert_eq!(
            epub_toc_chapter_index(&book, "Text/ch1.xhtml#part"),
            Some(0)
        );
        assert_eq!(epub_toc_chapter_index(&book, "ch2.xhtml"), Some(1));
        assert_eq!(epub_toc_chapter_index(&book, "missing.xhtml"), None);
    }

    #[test]
    fn resolves_managed_book_path_from_library_entry() {
        let temp = std::env::temp_dir().join(format!("epubreader-reader-test-{}", uuid::Uuid::new_v4()));
        let store = crate::storage::store::Store::new(Some(temp.clone()));
        let book_id = "book-1";
        let managed = store.paths.book_path(book_id, "epub");
        std::fs::write(&managed, b"epub").unwrap();
        store
            .save_library(&[crate::storage::store::LibraryEntry {
                id: book_id.to_string(),
                title: "Book".to_string(),
                author: "Author".to_string(),
                cover: None,
                file_path: "missing.epub".to_string(),
                added_at: 0,
                file_size: 4,
                format: "epub".to_string(),
            }])
            .unwrap();

        let resolved = resolve_book_path(&store, book_id).unwrap().unwrap();
        assert_eq!(resolved.0, managed);
        assert_eq!(resolved.1, "epub");
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn refuses_unsafe_book_id_before_touching_library() {
        let temp = std::env::temp_dir().join(format!("epubreader-reader-test-{}", uuid::Uuid::new_v4()));
        let store = crate::storage::store::Store::new(Some(temp.clone()));
        assert!(resolve_book_path(&store, "../escape").unwrap().is_none());
        std::fs::remove_dir_all(temp).unwrap();
    }
}
