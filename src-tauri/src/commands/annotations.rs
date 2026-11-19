//! 阅读批注的命令。业务逻辑在 `crate::db::annotations`。

use tauri::State;

use crate::app_state::AppState;
use crate::db::annotations::{self, Annotation, NewAnnotation};
use crate::error::Result;

#[tauri::command]
pub async fn annotation_list(state: State<'_, AppState>, article_id: i64) -> Result<Vec<Annotation>> {
    state.db.with_conn(|c| annotations::list(c, article_id))
}

#[tauri::command]
pub async fn annotation_add(state: State<'_, AppState>, input: NewAnnotation) -> Result<Annotation> {
    state.db.with_conn(|c| annotations::add(c, &input))
}

#[tauri::command]
pub async fn annotation_set_color(state: State<'_, AppState>, id: i64, color: String) -> Result<Annotation> {
    state.db.with_conn(|c| annotations::update(c, id, Some(&color), None))
}

/// `note` 为 null 或空白 = 清掉笔记
#[tauri::command]
pub async fn annotation_set_note(state: State<'_, AppState>, id: i64, note: Option<String>) -> Result<Annotation> {
    state.db.with_conn(|c| annotations::update(c, id, None, Some(note.as_deref())))
}

/// 删掉，返回删掉的那条（撤销时用它恢复）
#[tauri::command]
pub async fn annotation_delete(state: State<'_, AppState>, id: i64) -> Result<Option<Annotation>> {
    state.db.with_conn(|c| annotations::delete(c, id))
}
