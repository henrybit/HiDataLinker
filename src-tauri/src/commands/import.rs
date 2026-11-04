use crate::error::AppResult;
use crate::import::{
    build_create_table_ddl, cancel_import, import_job_status, infer_columns, parse_tabular_sample,
    prepare_import_script, stage_attachments, start_import, AttachmentMeta, ImportJobStatus,
    ImportPrepareRequest, ImportScriptPackage, ImportStartRequest, ParseSample,
    StageAttachmentsRequest, SuggestedColumn,
};
use crate::state::AppState;
use serde::Deserialize;
use std::path::PathBuf;
use tauri::{AppHandle, State};

#[tauri::command]
pub fn agent_stage_attachments(
    state: State<'_, AppState>,
    request: StageAttachmentsRequest,
) -> AppResult<Vec<AttachmentMeta>> {
    stage_attachments(state.chats_dir(), request)
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseSampleRequest {
    pub path: String,
    pub name: String,
}

#[tauri::command]
pub fn import_parse_sample(request: ParseSampleRequest) -> AppResult<ParseSample> {
    parse_tabular_sample(PathBuf::from(request.path.trim()).as_path(), &request.name)
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InferColumnsRequest {
    pub sample: ParseSample,
    pub engine: String,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InferColumnsResponse {
    pub columns: Vec<SuggestedColumn>,
    pub ddl: String,
    pub table: String,
    pub schema: String,
}

#[tauri::command]
pub fn import_infer_schema(
    request: InferColumnsRequest,
    schema: Option<String>,
    table: Option<String>,
) -> AppResult<InferColumnsResponse> {
    let engine = request.engine.trim();
    let columns = infer_columns(&request.sample, engine);
    let table_name = table
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(request.sample.suggested_table.as_str())
        .to_string();
    let schema_name = schema.unwrap_or_default();
    let ddl = build_create_table_ddl(engine, &schema_name, &table_name, &columns);
    Ok(InferColumnsResponse {
        columns,
        ddl,
        table: table_name,
        schema: schema_name,
    })
}

#[tauri::command]
pub fn import_prepare(
    state: State<'_, AppState>,
    request: ImportPrepareRequest,
) -> AppResult<ImportScriptPackage> {
    prepare_import_script(state.chats_dir(), request)
}

#[tauri::command]
pub fn import_start(
    app: AppHandle,
    request: ImportStartRequest,
) -> AppResult<ImportJobStatus> {
    start_import(app, request)
}

#[tauri::command]
pub fn import_cancel(job_id: String) -> AppResult<()> {
    cancel_import(job_id.trim())
}

#[tauri::command]
pub fn import_status(job_id: String) -> AppResult<Option<ImportJobStatus>> {
    Ok(import_job_status(job_id.trim()))
}

#[tauri::command]
pub fn import_read_text_file(path: String) -> AppResult<String> {
    let path = PathBuf::from(path.trim());
    if !path.is_file() {
        return Err(crate::error::AppError::msg("file not found"));
    }
    // Only allow reading under ~/.hidatalinker/
    let text = std::fs::read_to_string(&path)?;
    Ok(text)
}
