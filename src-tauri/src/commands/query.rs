use super::with_engine_as;
use crate::db::DatabaseEngine;
use crate::error::AppResult;
use crate::models::QueryResult;
use crate::sql_audit::AuditSource;
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub async fn preview_table(
    state: State<'_, AppState>,
    connection_id: String,
    schema: String,
    table: String,
    limit: u32,
    offset: u64,
) -> AppResult<QueryResult> {
    with_engine_as(&state, connection_id, AuditSource::TableBrowse, move |engine| async move {
        engine.preview_table(&schema, &table, limit, offset).await
    })
    .await
}

#[tauri::command]
pub async fn table_row_count(
    state: State<'_, AppState>,
    connection_id: String,
    schema: String,
    table: String,
) -> AppResult<u64> {
    with_engine_as(&state, connection_id, AuditSource::TableBrowse, move |engine| async move {
        engine.table_row_count(&schema, &table).await
    })
    .await
}

#[tauri::command]
pub async fn execute_sql(
    state: State<'_, AppState>,
    connection_id: String,
    schema: Option<String>,
    sql: String,
    source: Option<String>,
) -> AppResult<QueryResult> {
    let source = AuditSource::from_client(source.as_deref());
    with_engine_as(&state, connection_id, source, move |engine| async move {
        engine.execute_sql(schema.as_deref(), &sql).await
    })
    .await
}
