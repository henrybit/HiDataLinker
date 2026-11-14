use super::{with_engine_as, with_engine_tagged};
use crate::db::DatabaseEngine;
use crate::error::{AppError, AppResult};
use crate::models::{ExplainPlan, QueryResult};
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
pub async fn explain_plan(
    state: State<'_, AppState>,
    connection_id: String,
    schema: Option<String>,
    sql: String,
    analyze: bool,
    source: Option<String>,
    source_detail: Option<String>,
) -> AppResult<ExplainPlan> {
    let source = AuditSource::from_client(source.as_deref());
    with_engine_tagged(&state, connection_id, source, source_detail, move |engine| async move {
        engine.explain_plan(schema.as_deref(), &sql, analyze).await
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
    source_detail: Option<String>,
) -> AppResult<QueryResult> {
    let source = AuditSource::from_client(source.as_deref());
    with_engine_tagged(&state, connection_id, source, source_detail, move |engine| async move {
        engine.execute_sql(schema.as_deref(), &sql).await
    })
    .await
}

/// Events the drafting assistant may write to the SQL audit log. None of them touch the database.
const ASSIST_EVENTS: [&str; 2] = ["draft", "draft_applied"];

/// Audit that the assistant produced a draft, or that the user wrote one into an editor, so the
/// log tells drafted SQL apart from SQL the user ran.
#[tauri::command]
pub async fn record_assist_event(
    state: State<'_, AppState>,
    connection_id: String,
    schema: Option<String>,
    operation: String,
    sql: String,
    detail: Option<String>,
) -> AppResult<()> {
    if !ASSIST_EVENTS.contains(&operation.as_str()) {
        return Err(AppError::Message(format!("unknown assist event: {operation}")));
    }
    let engine = state
        .engine(&connection_id)?
        .with_source(AuditSource::Assist, detail);
    engine.note(&operation, schema.as_deref(), &sql);
    Ok(())
}
