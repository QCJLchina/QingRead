use crate::state::AppState;
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Serialize, Deserialize)]
pub struct ProgressData {
    pub book_id: String,
    pub chapter_index: usize,
    pub page_in_chapter: usize,
    pub total_pages_read: usize,
    pub last_read: u64,
}

#[tauri::command]
pub async fn save_progress(
    book_id: String,
    chapter_index: usize,
    page_in_chapter: usize,
    total_pages_read: usize,
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
    }))
}
