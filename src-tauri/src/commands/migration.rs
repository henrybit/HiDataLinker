use crate::error::AppResult;
use crate::migration_history::{
    delete_history, list_history, read_history, save_history, MigrationHistoryRecord,
    MigrationHistorySummary, NewMigrationHistory,
};
use crate::runtime::run_blocking;
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub async fn list_migration_history(
    state: State<'_, AppState>,
) -> AppResult<Vec<MigrationHistorySummary>> {
    let dir = state.migration_history_dir().clone();
    run_blocking(move || list_history(&dir)).await
}

#[tauri::command]
pub async fn read_migration_history(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<MigrationHistoryRecord> {
    let dir = state.migration_history_dir().clone();
    run_blocking(move || read_history(&dir, &id)).await
}

#[tauri::command]
pub async fn save_migration_history(
    state: State<'_, AppState>,
    request: NewMigrationHistory,
) -> AppResult<MigrationHistorySummary> {
    let dir = state.migration_history_dir().clone();
    run_blocking(move || save_history(&dir, request)).await
}

#[tauri::command]
pub async fn delete_migration_history(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let dir = state.migration_history_dir().clone();
    run_blocking(move || delete_history(&dir, &id)).await
}
