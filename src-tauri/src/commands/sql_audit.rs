use crate::error::AppResult;
use crate::runtime::run_blocking;
use crate::sql_audit::{SqlAuditDay, SqlAuditInfo, SqlAuditPage, SqlAuditQuery, SqlAuditSettings};
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub async fn read_sql_audit_settings(state: State<'_, AppState>) -> AppResult<SqlAuditInfo> {
    Ok(state.sql_audit().info())
}

#[tauri::command]
pub async fn write_sql_audit_settings(
    state: State<'_, AppState>,
    settings: SqlAuditSettings,
) -> AppResult<SqlAuditInfo> {
    let log = state.sql_audit().clone();
    run_blocking(move || {
        log.update_settings(settings)?;
        Ok(log.info())
    })
    .await
}

#[tauri::command]
pub async fn list_sql_audit(
    state: State<'_, AppState>,
    query: SqlAuditQuery,
) -> AppResult<SqlAuditPage> {
    let log = state.sql_audit().clone();
    run_blocking(move || log.query(&query)).await
}

#[tauri::command]
pub async fn list_sql_audit_days(state: State<'_, AppState>) -> AppResult<Vec<SqlAuditDay>> {
    let log = state.sql_audit().clone();
    run_blocking(move || log.list_days()).await
}
