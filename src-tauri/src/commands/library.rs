use crate::epub::parser::EpubParser;
use crate::epub::txt_parser::TxtParser;
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, State};

#[derive(Debug, Serialize, Deserialize)]
pub struct BookInfo {
    pub id: String,
    pub title: String,
    pub author: String,
    pub cover: Option<String>,
    pub file_path: String,
    pub added_at: u64,
    pub file_size: u64,
    pub format: String,
}

fn detect_format(path: &PathBuf) -> &'static str {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .map(|e| match e.as_str() {
            "txt" => "txt",
            "epub" => "epub",
            _ => "unknown",
        })
        .unwrap_or("unknown")
}

#[tauri::command]
pub async fn import_book(
    path: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<BookInfo, String> {
    let path = PathBuf::from(&path);

    if !path.exists() {
        let _ = app.emit("import-error", format!("文件不存在: {}", path.display()));
        return Err("File not found".to_string());
    }

    let format = detect_format(&path);
    if format == "unknown" {
        let _ = app.emit("import-error", "不支持的文件格式（仅支持 .epub 和 .txt）");
        return Err("Unsupported file format".to_string());
    }

    let _ = app.emit("import-progress", serde_json::json!({
        "stage": "解析文件",
        "file": path.file_name().and_then(|n| n.to_str()).unwrap_or(""),
    }));

    let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

    // 根据格式调用不同解析器
    let (title, author, cover) = if format == "epub" {
        let book_index = EpubParser::load_index(&path)
            .map_err(|e| {
                let _ = app.emit("import-error", format!("EPUB 解析失败: {}", e));
                format!("Failed to parse EPUB: {}", e)
            })?;
        (
            book_index.metadata.title.clone(),
            book_index.metadata.author.clone(),
            book_index.cover_data_uri.clone(),
        )
    } else {
        let txt = TxtParser::quick_scan(&path)
            .map_err(|e| {
                let _ = app.emit("import-error", format!("TXT 解析失败: {}", e));
                format!("Failed to parse TXT: {}", e)
            })?;
        (txt.title.clone(), txt.author.clone(), None::<String>)
    };

    let _ = app.emit("import-progress", serde_json::json!({
        "stage": "复制到数据目录",
        "file": path.file_name().and_then(|n| n.to_str()).unwrap_or(""),
    }));

    // 把书复制到数据目录 books/{id}.{ext}，不再依赖原路径。
    // NSIS 安装到 Program Files 后即使用户移动/删掉原文件，app 也能继续读。
    let new_id = uuid::Uuid::new_v4().to_string();
    let entry = {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        let dest = store.paths.book_path(&new_id, format);
        if let Some(parent) = dest.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::copy(&path, &dest) {
            let _ = app.emit(
                "import-error",
                format!("复制文件到数据目录失败: {} -> {} ({})", path.display(), dest.display(), e),
            );
            return Err(format!("Failed to copy book into data dir: {}", e));
        }

        let metadata = crate::epub::parser::BookMetadata {
            title: title.clone(),
            author: author.clone(),
            language: String::new(),
            publisher: String::new(),
            description: String::new(),
            cover_href: None,
        };

        let mut entry = match store.add_book_with_id(
            &new_id,
            metadata,
            &dest.to_string_lossy(),
            file_size,
            format,
        ) {
            Ok(entry) => entry,
            Err(e) => {
                let _ = std::fs::remove_file(&dest);
                let _ = app.emit("import-error", format!("添加失败: {}", e));
                return Err(format!("Failed to add book: {}", e));
            }
        };

        // 保存封面到数据目录
        if let Some(cover_data) = cover {
            if store.update_book_cover(&entry.id, &cover_data).is_ok() {
                if let Some(updated) = store
                    .load_library_checked()
                    .map_err(|e| format!("Failed to reload library: {}", e))?
                    .into_iter()
                    .find(|e| e.id == entry.id)
                {
                    entry = updated;
                }
            }
        }

        entry
    };

    let _ = app.emit("import-complete", &entry.id);

    Ok(BookInfo {
        id: entry.id,
        title: entry.title,
        author: entry.author,
        cover: entry.cover,
        file_path: entry.file_path,
        added_at: entry.added_at,
        file_size: entry.file_size,
        format: entry.format,
    })
}

#[tauri::command]
pub async fn list_books(state: State<'_, AppState>) -> Result<Vec<BookInfo>, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let library = store
        .load_library_checked()
        .map_err(|e| format!("Failed to load library: {}", e))?;

    Ok(library.into_iter().map(|e| BookInfo {
        id: e.id,
        title: e.title,
        author: e.author,
        cover: e.cover,
        file_path: e.file_path,
        added_at: e.added_at,
        file_size: e.file_size,
        format: if e.format.is_empty() { "epub".to_string() } else { e.format },
    }).collect())
}

#[tauri::command]
pub async fn remove_book(book_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    store.remove_book(&book_id).map_err(|e| format!("Failed to remove book: {}", e))
}

#[tauri::command]
pub async fn batch_remove_books(book_ids: Vec<String>, state: State<'_, AppState>) -> Result<(), String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let mut failures = Vec::new();
    for book_id in &book_ids {
        if let Err(e) = store.remove_book(book_id) {
            failures.push(format!("{}: {}", book_id, e));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Failed to remove books: {}",
            failures.join("; ")
        ))
    }
}

#[tauri::command]
pub async fn get_cover(book_id: String, state: State<'_, AppState>) -> Result<Option<String>, String> {
    if !crate::storage::paths::is_safe_book_id(&book_id) {
        return Ok(None);
    }
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let library = store
        .load_library_checked()
        .map_err(|e| format!("Failed to load library: {}", e))?;

    if let Some(entry) = library.iter().find(|e| e.id == book_id) {
        if let Some(ref cover) = entry.cover {
            return Ok(Some(cover.clone()));
        }

        let format = if entry.format.is_empty() { "epub" } else { &entry.format };
        let book_path = store.paths.book_path(&book_id, format);

        if book_path.exists() && format == "epub" {
            if let Ok(Some(cover)) = EpubParser::extract_cover(&book_path) {
                let _ = store.update_book_cover(&book_id, &cover);
                return Ok(Some(cover));
            }
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_supported_book_formats_case_insensitively() {
        assert_eq!(detect_format(&PathBuf::from("book.EPUB")), "epub");
        assert_eq!(detect_format(&PathBuf::from("book.Txt")), "txt");
        assert_eq!(detect_format(&PathBuf::from("book.pdf")), "unknown");
        assert_eq!(detect_format(&PathBuf::from("book")), "unknown");
    }
}
