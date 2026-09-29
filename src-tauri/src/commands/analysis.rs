use crate::analysis_history::{
    delete_history, list_history, read_history, save_history, AnalysisHistoryRecord,
    AnalysisHistorySummary, NewAnalysisHistory,
};
use crate::error::AppResult;
use crate::runtime::run_blocking;
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub async fn list_analysis_history(
    state: State<'_, AppState>,
) -> AppResult<Vec<AnalysisHistorySummary>> {
    let dir = state.analysis_history_dir().clone();
    run_blocking(move || list_history(&dir)).await
}

#[tauri::command]
pub async fn read_analysis_history(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<AnalysisHistoryRecord> {
    let dir = state.analysis_history_dir().clone();
    run_blocking(move || read_history(&dir, &id)).await
}

#[tauri::command]
pub async fn save_analysis_history(
    state: State<'_, AppState>,
    request: NewAnalysisHistory,
) -> AppResult<AnalysisHistorySummary> {
    let dir = state.analysis_history_dir().clone();
    run_blocking(move || save_history(&dir, request)).await
}

#[tauri::command]
pub async fn delete_analysis_history(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let dir = state.analysis_history_dir().clone();
    run_blocking(move || delete_history(&dir, &id)).await
}
