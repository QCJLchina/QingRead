use crate::state::AppState;
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Serialize, Deserialize)]
pub struct ProgressSummary {
    pub book_id: String,
    pub chapter_index: usize,
    pub total_pages_read: usize,
    pub last_read: u64,
}

/// 书架需要「最近在读」和进度百分比，这里一次性把全部进度读出来。
#[tauri::command]
pub async fn list_progress(state: State<'_, AppState>) -> Result<Vec<ProgressSummary>, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    Ok(store
        .load_all_progress()
        .into_iter()
        .map(|p| ProgressSummary {
            book_id: p.book_id,
            chapter_index: p.chapter_index,
            total_pages_read: p.total_pages_read,
            last_read: p.last_read,
        })
        .collect())
}


#[derive(Debug, Serialize, Deserialize)]
pub struct ProgressData {
    pub book_id: String,
    pub chapter_index: usize,
    pub page_in_chapter: usize,
    pub total_pages_read: usize,
    pub last_read: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

/// 保存阅读进度。命令名与参数名保持历史形状（page_in_chapter / total_pages_read），
/// anchor 与 mode 是可选新增字段：老前端不传也能正常工作，老进度文件读取时按页码恢复。
#[tauri::command]
pub async fn save_progress(
    book_id: String,
    chapter_index: usize,
    page_in_chapter: usize,
    total_pages_read: usize,
    anchor: Option<serde_json::Value>,
    mode: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let progress = crate::storage::store::ReadingProgress {
        book_id,
        chapter_index,
        page_in_chapter,
        total_pages_read,
        last_read: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        anchor,
        mode,
    };
    store.save_progress(&progress).map_err(|e| format!("Failed to save progress: {}", e))
}

#[tauri::command]
pub async fn load_progress(
    book_id: String,
    state: State<'_, AppState>,
) -> Result<Option<ProgressData>, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let progress = store.load_progress(&book_id);
    Ok(progress.map(|p| ProgressData {
        book_id: p.book_id,
        chapter_index: p.chapter_index,
        page_in_chapter: p.page_in_chapter,
        total_pages_read: p.total_pages_read,
        last_read: p.last_read,
        anchor: p.anchor,
        mode: p.mode,
    }))
}
