//! 阅读周报：记阅读时长、按周汇总（见 db/weekly.rs）

use tauri::State;

use crate::app_state::AppState;
use crate::db::weekly::{self, WeeklyReport};
use crate::error::Result;

/// 阅读页每 30 秒（和离开时）记一次：今天这篇读了几秒、往前读了多少词
#[tauri::command]
pub async fn reading_log_add(state: State<'_, AppState>, day: String, article_id: i64, seconds: i64, words: i64) -> Result<()> {
    state.db.with_conn(|c| weekly::add_reading(c, &day, article_id, seconds, words))
}

/// 一周的汇总：日期 [from_day, to_day)，毫秒 [from_ms, to_ms)（都按本地时间，由前端算）
#[tauri::command]
pub async fn weekly_report(state: State<'_, AppState>, from_day: String, to_day: String, from_ms: i64, to_ms: i64) -> Result<WeeklyReport> {
    state.db.with_conn(|c| weekly::weekly_report(c, &from_day, &to_day, from_ms, to_ms))
}

/// 存周报卡片：前端 Canvas 导出的 PNG（base64）写到用户选的路径
#[tauri::command]
pub async fn weekly_save_png(path: std::path::PathBuf, base64: String) -> Result<()> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64.trim_start_matches("data:image/png;base64,"))
        .map_err(|e| crate::error::Error::Other(e.to_string()))?;
    std::fs::write(&path, bytes)?;
    Ok(())
}
