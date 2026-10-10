use crate::state::AppState;
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Serialize, Deserialize)]
pub struct BookmarkData {
    pub id: String,
    pub chapter_index: usize,
    pub anchor: serde_json::Value,
    pub snippet: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub created_at: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HistoryEntryData {
    pub id: String,
    pub kind: String,
    pub chapter_index: usize,
    pub anchor: serde_json::Value,
    pub snippet: String,
    pub created_at: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReadingDataPayload {
    pub book_id: String,
    pub status: String,
    pub bookmarks: Vec<BookmarkData>,
    pub history: Vec<HistoryEntryData>,
    pub updated_at: u64,
}

impl ReadingDataPayload {
    fn from_store(data: crate::storage::store::ReadingData) -> Self {
        Self {
            book_id: data.book_id,
            status: data.status,
            bookmarks: data
                .bookmarks
                .into_iter()
                .map(|b| BookmarkData {
                    id: b.id,
                    chapter_index: b.chapter_index,
                    anchor: b.anchor,
                    snippet: b.snippet,
                    note: b.note,
                    created_at: b.created_at,
                })
                .collect(),
            history: data
                .history
                .into_iter()
                .map(|h| HistoryEntryData {
                    id: h.id,
                    kind: h.kind,
                    chapter_index: h.chapter_index,
                    anchor: h.anchor,
                    snippet: h.snippet,
                    created_at: h.created_at,
                })
                .collect(),
            updated_at: data.updated_at,
        }
    }
}

/// 书架筛选需要的汇总：状态 + 书签数 + 历史数 + 更新时间。
#[derive(Debug, Serialize, Deserialize)]
pub struct ReadingSummary {
    pub book_id: String,
    pub status: String,
    pub bookmark_count: usize,
    pub history_count: usize,
    pub updated_at: u64,
}

fn anchor_of(value: Option<serde_json::Value>) -> serde_json::Value {
    value.unwrap_or_else(|| serde_json::json!({ "path": [], "offset": 0, "snippet": "", "ratio": 0 }))
}

#[tauri::command]
pub async fn get_reading_data(
    book_id: String,
    state: State<'_, AppState>,
) -> Result<ReadingDataPayload, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let data = store
        .load_reading_data(&book_id)
        .map_err(|e| format!("读取阅读资料失败: {}", e))?;
    Ok(ReadingDataPayload::from_store(data))
}

#[tauri::command]
pub async fn list_reading_summaries(
    state: State<'_, AppState>,
) -> Result<Vec<ReadingSummary>, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    Ok(store
        .load_all_reading_summaries()
        .into_iter()
        .map(|data| ReadingSummary {
            book_id: data.book_id,
            status: data.status,
            bookmark_count: data.bookmarks.len(),
            history_count: data.history.len(),
            updated_at: data.updated_at,
        })
        .collect())
}

#[tauri::command]
pub async fn set_reading_status(
    book_id: String,
    status: String,
    state: State<'_, AppState>,
) -> Result<ReadingDataPayload, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let data = store
        .set_reading_status(&book_id, &status)
        .map_err(|e| format!("保存阅读状态失败: {}", e))?;
    Ok(ReadingDataPayload::from_store(data))
}

#[derive(Debug, Serialize)]
pub struct AddBookmarkResult {
    data: ReadingDataPayload,
    /// false 表示该位置已有书签，本次没有新增
    created: bool,
}

#[tauri::command]
pub async fn mark_reading_started(
    book_id: String,
    state: State<'_, AppState>,
) -> Result<ReadingDataPayload, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    store.mark_reading_started(&book_id)
        .map(ReadingDataPayload::from_store)
        .map_err(|e| format!("保存阅读状态失败: {}", e))
}

#[tauri::command]
pub async fn add_bookmark(
    book_id: String,
    chapter_index: usize,
    anchor: Option<serde_json::Value>,
    snippet: String,
    note: Option<String>,
    state: State<'_, AppState>,
) -> Result<AddBookmarkResult, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let (data, created) = store
        .add_bookmark(&book_id, chapter_index, anchor_of(anchor), snippet, note)
        .map_err(|e| format!("保存书签失败: {}", e))?;
    Ok(AddBookmarkResult {
        data: ReadingDataPayload::from_store(data),
        created,
    })
}

#[tauri::command]
pub async fn update_bookmark(
    book_id: String,
    bookmark_id: String,
    note: Option<String>,
    state: State<'_, AppState>,
) -> Result<ReadingDataPayload, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let data = store
        .update_bookmark_note(&book_id, &bookmark_id, note)
        .map_err(|e| format!("更新书签失败: {}", e))?;
    Ok(ReadingDataPayload::from_store(data))
}

#[tauri::command]
pub async fn delete_bookmark(
    book_id: String,
    bookmark_id: String,
    state: State<'_, AppState>,
) -> Result<ReadingDataPayload, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let data = store
        .remove_bookmark(&book_id, &bookmark_id)
        .map_err(|e| format!("删除书签失败: {}", e))?;
    Ok(ReadingDataPayload::from_store(data))
}

#[tauri::command]
pub async fn append_history(
    book_id: String,
    kind: String,
    chapter_index: usize,
    anchor: Option<serde_json::Value>,
    snippet: String,
    state: State<'_, AppState>,
) -> Result<ReadingDataPayload, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let data = store
        .append_history(&book_id, &kind, chapter_index, anchor_of(anchor), snippet)
        .map_err(|e| format!("保存阅读历史失败: {}", e))?;
    Ok(ReadingDataPayload::from_store(data))
}

#[tauri::command]
pub async fn clear_history(
    book_id: String,
    state: State<'_, AppState>,
) -> Result<ReadingDataPayload, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let data = store
        .clear_history(&book_id)
        .map_err(|e| format!("清空阅读历史失败: {}", e))?;
    Ok(ReadingDataPayload::from_store(data))
}
