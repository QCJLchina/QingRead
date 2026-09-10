#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod epub;
mod hotkey;
mod state;
mod storage;
mod sync;
mod tray;

use state::AppState;
use storage::store::LibraryEntry;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::http::{Request, Response};
use tauri::Manager;

struct LruCache {
    map: HashMap<String, (Vec<u8>, String)>,
    order: Vec<String>,
    max_entries: usize,
    max_bytes: usize,
    total_bytes: usize,
}

impl LruCache {
    fn new(max_entries: usize, max_bytes: usize) -> Self {
        Self {
            map: HashMap::new(),
            order: Vec::new(),
            max_entries,
            max_bytes,
            total_bytes: 0,
        }
    }

    fn get(&mut self, key: &str) -> Option<(Vec<u8>, String)> {
        if let Some((bytes, mime)) = self.map.get(key) {
            let result = (bytes.clone(), mime.clone());
            if let Some(pos) = self.order.iter().position(|k| k == key) {
                self.order.remove(pos);
            }
            self.order.push(key.to_string());
            Some(result)
        } else {
            None
        }
    }

    fn insert(&mut self, key: String, value: (Vec<u8>, String)) {
        let value_bytes = value.0.len();
        if let Some((old_bytes, _)) = self.map.get(&key) {
            self.total_bytes = self.total_bytes.saturating_sub(old_bytes.len());
            if let Some(pos) = self.order.iter().position(|k| k == &key) {
                self.order.remove(pos);
            }
        }
        self.map.insert(key.clone(), value);
        self.total_bytes += value_bytes;
        self.order.push(key);

        while (self.map.len() > self.max_entries || self.total_bytes > self.max_bytes)
            && !self.order.is_empty()
        {
            let old_key = self.order.remove(0);
            if let Some((old_bytes, _)) = self.map.remove(&old_key) {
                self.total_bytes = self.total_bytes.saturating_sub(old_bytes.len());
            }
        }
    }
}

static EPUB_ASSET_CACHE: Lazy<Mutex<LruCache>> =
    Lazy::new(|| Mutex::new(LruCache::new(200, 64 * 1024 * 1024)));

/// 窗口拖动/缩放的防抖序号。连续事件只保留最后一次写入。
static LAYOUT_SAVE_GENERATION: Lazy<Mutex<u64>> = Lazy::new(|| Mutex::new(0));

/// 用户拖动窗口边框或移动窗口后，延迟记录最终的尺寸与位置。
/// 拖动过程中事件非常密集，这里用序号做防抖：只有最后一次落盘。
fn schedule_layout_save(app: &tauri::AppHandle) {
    let generation = {
        let mut guard = match LAYOUT_SAVE_GENERATION.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        *guard += 1;
        *guard
    };

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(600)).await;
        let is_latest = LAYOUT_SAVE_GENERATION
            .lock()
            .map(|guard| *guard == generation)
            .unwrap_or(false);
        if !is_latest {
            return;
        }
        if let Err(e) = commands::window::persist_current_layout(&app) {
            eprintln!("[window] 保存窗口形态失败: {e}");
        }
    });
}

pub fn clear_epub_asset_cache() {
    let mut cache = EPUB_ASSET_CACHE.lock().unwrap();
    cache.map.clear();
    cache.order.clear();
    cache.total_bytes = 0;
}

fn main() {
    // 启动时从默认数据目录的 settings.json 读 data_dir，
    // 这样用户之前在设置里改过目录后，重启应用仍然能落到原位置。
    let initial_data_dir = read_persisted_data_dir();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .manage(AppState::new(initial_data_dir))
        // 自定义协议 `epub:///<book_id>/<path>` 让 webview 按需从 zip 里读资源
        .register_uri_scheme_protocol("epub-asset", move |ctx, request| {
            handle_epub_asset_protocol(ctx, request)
        })
        .register_uri_scheme_protocol("bg-asset", move |ctx, request| {
            handle_bg_asset_protocol(ctx, request)
        })
        .invoke_handler(tauri::generate_handler![
            // Library
            commands::library::import_book,
            commands::library::list_books,
            commands::library::remove_book,
            commands::library::batch_remove_books,
            commands::library::get_cover,
            // Reader
            commands::reader::open_reader,
            commands::reader::load_chapter,
            commands::reader::get_toc,
            commands::reader::get_chapter_offsets,
            // Progress
            commands::progress::save_progress,
            commands::progress::load_progress,
            commands::progress::list_progress,
            // Settings
            commands::settings::get_settings,
            commands::settings::save_settings,
            commands::settings::set_data_dir,
            commands::settings::restart_app,
            // Sync
            commands::sync::get_sync_config,
            commands::sync::set_sync_config,
            commands::sync::clear_sync_config,
            commands::sync::test_sync_connection,
            commands::sync::preview_sync,
            commands::sync::apply_sync,
            // Window / 低干扰模式
            commands::window::get_window_settings,
            commands::window::save_window_settings,
            commands::window::window_apply_layout,
            commands::window::window_hide,
            // FS
            commands::fs_commands::reveal_in_folder,
            commands::fs_commands::get_app_version,
        ])
        .setup(|app| {
            tray::setup_tray(app)?;

            // 先套用上次保存的窗口形态，再显示窗口，避免启动时先闪一个默认尺寸。
            let window_settings = commands::window::apply_saved_layout_on_startup(app.handle());
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }

            // 全局隐藏/恢复快捷键。注册失败只会没有快捷键，托盘菜单仍然可用。
            hotkey::set_hotkey(app.handle().clone(), window_settings.hide_hotkey.clone());

            // 处理 CLI 启动参数：自动导入传入的 EPUB/TXT
            let args: Vec<String> = std::env::args().skip(1).collect();
            for arg in args {
                let lower = arg.to_lowercase();
                if !lower.ends_with(".epub") && !lower.ends_with(".txt") {
                    continue;
                }
                let path = std::path::PathBuf::from(&arg);
                if !path.exists() {
                    continue;
                }
                let state = app.state::<AppState>();
                if let Ok(store) = state.store.lock() {
                    if lower.ends_with(".epub") {
                        if let Ok(book_index) = crate::epub::parser::EpubParser::load_index(&path) {
                            let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                            let _ = store.add_book_with_format(
                                book_index.metadata.clone(),
                                &path.to_string_lossy(),
                                file_size,
                                "epub",
                            );
                        }
                    } else if let Ok(txt) = crate::epub::txt_parser::TxtParser::quick_scan(&path) {
                        let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                        let metadata = crate::epub::parser::BookMetadata {
                            title: txt.title.clone(),
                            author: txt.author.clone(),
                            language: String::new(),
                            publisher: String::new(),
                            description: String::new(),
                            cover_href: None,
                        };
                        let _ = store.add_book_with_format(
                            metadata,
                            &path.to_string_lossy(),
                            file_size,
                            "txt",
                        );
                    }
                };
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if matches!(
                    event,
                    tauri::WindowEvent::Resized(_) | tauri::WindowEvent::Moved(_)
                ) {
                    schedule_layout_save(window.app_handle());
                }
            }

            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    // 读取用户设置的关闭行为
                    let behavior = {
                        let state = window.state::<AppState>();
                        state
                            .store
                            .lock()
                            .ok()
                            .map(|s| s.load_settings().close_behavior)
                            .unwrap_or_else(|| "quit".to_string())
                    };

                    // 退出前先记录窗口形态，下次启动能回到同一个位置和大小
                    let _ = commands::window::persist_current_layout(window.app_handle());

                    match behavior.as_str() {
                        "minimize_to_tray" => {
                            // 最小化到托盘
                            let _ = window.hide();
                            api.prevent_close();
                        }
                        _ => {
                            // 直接退出（默认）：移除托盘图标后退出，避免残留
                            if let Some(tray) = window.app_handle().tray_by_id("main-tray") {
                                let _ = tray.set_visible(false);
                            }
                            window.app_handle().exit(0);
                        }
                    }
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// 启动时从默认数据目录的 settings.json 读 data_dir。
/// 注意：这里要使用「默认」数据目录的 settings.json（与 `AppPaths::new(None)` 一致），
/// 不能用用户自定义目录，否则每次启动都会读到上次的设置并反复跳。
fn read_persisted_data_dir() -> Option<std::path::PathBuf> {
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct OnlyDataDir {
        data_dir: Option<String>,
    }

    let default_base = dirs::data_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from(".")))
        .join("EpubReader");
    let settings_file = default_base.join("settings.json");
    let Ok(text) = std::fs::read_to_string(&settings_file) else {
        return None;
    };
    let parsed: OnlyDataDir = serde_json::from_str(&text).ok()?;
    parsed
        .data_dir
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
}
/// 从 zip 里读资源并返回。不内联到 HTML，避免大图导致章节加载卡死。
/// Windows 上 Tauri 把 scheme 映射为 `http://<scheme>.localhost/...`，
/// 所以 URI 形如 `http://epub-asset.localhost/<encoded-source-path>/<encoded-entry>`。
fn handle_epub_asset_protocol<R: tauri::Runtime>(
    ctx: tauri::UriSchemeContext<'_, R>,
    request: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    let path_in_url = request.uri().path().to_string();
    // 去掉开头的 '/'
    let stripped = path_in_url.trim_start_matches('/');
    let (encoded_path, encoded_entry) = match stripped.split_once('/') {
        Some((p, e)) => (p.to_string(), e.to_string()),
        None => (stripped.to_string(), String::new()),
    };
    let source_path = percent_encoding::percent_decode_str(&encoded_path)
        .decode_utf8_lossy()
        .to_string();
    let entry_path = percent_encoding::percent_decode_str(&encoded_entry)
        .decode_utf8_lossy()
        .to_string();

    if !is_safe_zip_entry(&entry_path) {
        return not_found();
    }

    let allowed = {
        let app = ctx.app_handle();
        let state = app.state::<AppState>();
        let result = match state.store.lock() {
            Ok(store) => store
                .load_library_checked()
                .map(|library| {
                    is_allowed_source_path(
                        std::path::Path::new(&source_path),
                        &store.paths.books_dir,
                        &library,
                    )
                })
                .unwrap_or(false),
            Err(_) => false,
        };
        result
    };
    if !allowed {
        return not_found();
    }

    let cache_key = format!("{}::{}", source_path, entry_path);
    {
        let mut cache = EPUB_ASSET_CACHE.lock().unwrap();
        if let Some((bytes, mime)) = cache.get(&cache_key) {
            return Response::builder()
                .status(200)
                .header("Content-Type", mime)
                .body(bytes)
                .unwrap();
        }
    }

    if source_path.is_empty() || !std::path::Path::new(&source_path).exists() {
        return not_found();
    }

    let file = match std::fs::File::open(&source_path) {
        Ok(f) => f,
        Err(_) => return not_found(),
    };
    let mut archive = match zip::ZipArchive::new(file) {
        Ok(a) => a,
        Err(_) => return not_found(),
    };
    let entry_names: Vec<String> = (0..archive.len())
        .filter_map(|i| archive.by_index(i).ok().map(|entry| entry.name().to_string()))
        .collect();
    let idx = match find_entry_index(&entry_names, &entry_path) {
        Some(i) => i,
        None => return not_found(),
    };

    let zip_entry = match archive.by_index(idx) {
        Ok(e) => e,
        Err(_) => return not_found(),
    };
    let mime = guess_mime_from_name(zip_entry.name());
    let mut bytes = Vec::new();
    use std::io::Read;
    const MAX_ENTRY_BYTES: u64 = 64 * 1024 * 1024;
    let mut limited = zip_entry.take(MAX_ENTRY_BYTES + 1);
    if limited.read_to_end(&mut bytes).is_err() || bytes.len() as u64 > MAX_ENTRY_BYTES {
        return not_found();
    }

    const MAX_CACHE_BYTES: usize = 8 * 1024 * 1024;
    if bytes.len() <= MAX_CACHE_BYTES {
        let mut cache = EPUB_ASSET_CACHE.lock().unwrap();
        cache.insert(cache_key, (bytes.clone(), mime.clone()));
    }

    Response::builder()
        .status(200)
        .header("Content-Type", mime)
        .header("Cache-Control", "public, max-age=86400")
        .body(bytes)
        .unwrap()
}

fn handle_bg_asset_protocol<R: tauri::Runtime>(
    ctx: tauri::UriSchemeContext<'_, R>,
    request: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    let path_in_url = request.uri().path().to_string();
    let decoded_path = percent_encoding::percent_decode_str(path_in_url.trim_start_matches('/'))
        .decode_utf8_lossy()
        .to_string();
    let requested_path = std::path::PathBuf::from(&decoded_path);

    let allowed = {
        let app = ctx.app_handle();
        let state = app.state::<AppState>();
        let result = match state.store.lock() {
            Ok(store) => is_allowed_background_path(
                store.load_settings().custom_bg_image.as_deref(),
                &requested_path,
            ),
            Err(_) => false,
        };
        result
    };
    if !allowed {
        return not_found();
    }

    let Ok(metadata) = std::fs::metadata(&requested_path) else {
        return not_found();
    };
    if !metadata.is_file() {
        return not_found();
    }

    const MAX_BACKGROUND_BYTES: u64 = 50 * 1024 * 1024;
    if metadata.len() > MAX_BACKGROUND_BYTES {
        return not_found();
    }
    let Ok(bytes) = std::fs::read(&requested_path) else {
        return not_found();
    };

    Response::builder()
        .status(200)
        .header("Content-Type", guess_mime_from_name(&decoded_path))
        .header("Cache-Control", "public, max-age=86400")
        .body(bytes)
        .unwrap()
}

fn is_allowed_background_path(
    configured: Option<&str>,
    requested_path: &std::path::Path,
) -> bool {
    let Some(configured) = configured else {
        return false;
    };
    let Ok(configured_path) = std::path::Path::new(configured).canonicalize() else {
        return false;
    };
    let Ok(requested) = requested_path.canonicalize() else {
        return false;
    };
    requested == configured_path
}

fn find_entry_index(entry_names: &[String], entry_path: &str) -> Option<usize> {
    if let Some(i) = entry_names.iter().position(|name| name == entry_path) {
        return Some(i);
    }

    let basename = std::path::Path::new(entry_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(entry_path);
    let matches: Vec<usize> = entry_names
        .iter()
        .enumerate()
        .filter_map(|(i, name)| {
            let name_basename = std::path::Path::new(name)
                .file_name()
                .and_then(|n| n.to_str());
            if name_basename == Some(basename) {
                Some(i)
            } else {
                None
            }
        })
        .collect();

    if matches.len() == 1 {
        Some(matches[0])
    } else {
        None
    }
}

fn is_safe_zip_entry(entry_path: &str) -> bool {
    if entry_path.is_empty() || entry_path.contains('\0') {
        return false;
    }
    let path = std::path::Path::new(entry_path);
    if path.is_absolute() {
        return false;
    }
    for component in path.components() {
        match component {
            std::path::Component::Prefix(_)
            | std::path::Component::RootDir
            | std::path::Component::ParentDir => return false,
            std::path::Component::CurDir | std::path::Component::Normal(_) => {}
        }
    }
    true
}

fn is_allowed_source_path(
    source_path: &std::path::Path,
    books_dir: &std::path::Path,
    library: &[LibraryEntry],
) -> bool {
    if !source_path.is_file() {
        return false;
    }
    let Ok(source) = source_path.canonicalize() else {
        return false;
    };
    if let Ok(books) = books_dir.canonicalize() {
        if source.starts_with(&books) {
            return true;
        }
    }
    library.iter().any(|entry| {
        std::path::Path::new(&entry.file_path)
            .canonicalize()
            .map(|candidate| candidate == source)
            .unwrap_or(false)
    })
}

fn not_found() -> Response<Vec<u8>> {
    Response::builder().status(404).body(Vec::new()).unwrap()
}

fn guess_mime_from_name(name: &str) -> String {
    let lower = name.to_lowercase();
    if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg".to_string()
    } else if lower.ends_with(".png") {
        "image/png".to_string()
    } else if lower.ends_with(".gif") {
        "image/gif".to_string()
    } else if lower.ends_with(".webp") {
        "image/webp".to_string()
    } else if lower.ends_with(".svg") {
        "image/svg+xml".to_string()
    } else if lower.ends_with(".css") {
        "text/css".to_string()
    } else if lower.ends_with(".js") {
        "application/javascript".to_string()
    } else if lower.ends_with(".woff") {
        "font/woff".to_string()
    } else if lower.ends_with(".woff2") {
        "font/woff2".to_string()
    } else if lower.ends_with(".ttf") {
        "font/ttf".to_string()
    } else if lower.ends_with(".otf") {
        "font/otf".to_string()
    } else {
        "application/octet-stream".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_protocol_accepts_only_configured_path() {
        let temp_dir = std::env::temp_dir().join(format!(
            "qingread-bg-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let selected = temp_dir.join("bg.png");
        let other = temp_dir.join("other.png");
        std::fs::write(&selected, b"image").unwrap();
        std::fs::write(&other, b"image").unwrap();
        let selected_str = selected.to_str().unwrap();

        assert!(is_allowed_background_path(
            Some(selected_str),
            &selected
        ));
        assert!(!is_allowed_background_path(
            Some(selected_str),
            &other
        ));
        assert!(!is_allowed_background_path(None, &selected));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn zip_entry_rejects_traversal_and_absolute_paths() {
        assert!(!is_safe_zip_entry(""));
        assert!(!is_safe_zip_entry("..\\evil"));
        assert!(!is_safe_zip_entry("../evil"));
        assert!(!is_safe_zip_entry("OEBPS/../../evil"));
        assert!(!is_safe_zip_entry("/etc/passwd"));
        assert!(!is_safe_zip_entry("C:\\Windows\\win.ini"));
        assert!(!is_safe_zip_entry("a\0b"));
    }

    #[test]
    fn zip_entry_accepts_epub_relative_paths() {
        assert!(is_safe_zip_entry("OEBPS/chapter1.xhtml"));
        assert!(is_safe_zip_entry("images/cover.jpg"));
        assert!(is_safe_zip_entry("./images/cover.jpg"));
    }

    #[test]
    fn source_path_must_be_managed_or_registered() {
        let temp_dir = std::env::temp_dir().join(format!(
            "qingread-path-test-{}",
            uuid::Uuid::new_v4()
        ));
        let books_dir = temp_dir.join("books");
        std::fs::create_dir_all(&books_dir).unwrap();

        let managed = books_dir.join("managed.epub");
        std::fs::write(&managed, b"managed").unwrap();
        let registered = temp_dir.join("registered.epub");
        std::fs::write(&registered, b"registered").unwrap();
        let unregistered = temp_dir.join("unregistered.epub");
        std::fs::write(&unregistered, b"unregistered").unwrap();

        let entry = LibraryEntry {
            id: "book-1".to_string(),
            title: "Book".to_string(),
            author: String::new(),
            cover: None,
            file_path: registered.to_string_lossy().to_string(),
            added_at: 0,
            file_size: 10,
            format: "epub".to_string(),
        };

        assert!(is_allowed_source_path(&managed, &books_dir, &[entry.clone()]));
        assert!(is_allowed_source_path(&registered, &books_dir, &[entry.clone()]));
        assert!(!is_allowed_source_path(&unregistered, &books_dir, &[entry]));
        assert!(!is_allowed_source_path(&temp_dir.join("missing.epub"), &books_dir, &[]));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn lru_cache_evicts_by_total_bytes() {
        let mut cache = LruCache::new(10, 10);
        cache.insert("a".to_string(), (vec![1; 6], "text/plain".to_string()));
        cache.insert("b".to_string(), (vec![2; 6], "text/plain".to_string()));

        assert!(!cache.map.contains_key("a"));
        assert!(cache.map.contains_key("b"));
        assert_eq!(cache.total_bytes, 6);
        assert_eq!(cache.map.len(), 1);
    }

    #[test]
    fn lru_cache_still_evicts_by_entry_count() {
        let mut cache = LruCache::new(2, 1024);
        cache.insert("a".to_string(), (vec![1; 2], "text/plain".to_string()));
        cache.insert("b".to_string(), (vec![2; 2], "text/plain".to_string()));
        cache.insert("c".to_string(), (vec![3; 2], "text/plain".to_string()));

        assert!(!cache.map.contains_key("a"));
        assert!(cache.map.contains_key("b"));
        assert!(cache.map.contains_key("c"));
    }

    #[test]
    fn resource_entry_prefers_exact_path_over_basename() {
        let names = vec![
            "OEBPS/images/cover.jpg".to_string(),
            "images/cover.jpg".to_string(),
        ];

        assert_eq!(find_entry_index(&names, "images/cover.jpg"), Some(1));
    }

    #[test]
    fn resource_entry_rejects_ambiguous_basename() {
        let names = vec!["a/cover.jpg".to_string(), "b/cover.jpg".to_string()];

        assert_eq!(find_entry_index(&names, "images/cover.jpg"), None);
    }

    #[test]
    fn resource_entry_matches_unique_basename() {
        let names = vec!["OEBPS/images/cover.jpg".to_string()];

        assert_eq!(find_entry_index(&names, "images/cover.jpg"), Some(0));
    }
}
