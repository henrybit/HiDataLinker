use crate::agent::catalog::{self, ToolError};
use crate::builtin_agents::{
    append_transcript, create_session, list_agents, list_sessions, load_session, read_system_prompt,
    save_checkpoint, AgentInfo, AgentRunner, AgentRunnerStatus, AgentSessionDetail,
    AgentSessionMeta, AgentStartRunRequest, AgentStartRunResponse, NewAgentSession, TranscriptEntry,
};
use crate::error::{AppError, AppResult};
use crate::runtime::run_blocking;
use crate::state::AppState;
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;
use tauri::State;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentToolResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[tauri::command]
pub async fn agents_list(state: State<'_, AppState>) -> AppResult<Vec<AgentInfo>> {
    let dir = state.agents_dir().clone();
    run_blocking(move || list_agents(&dir)).await
}

#[tauri::command]
pub async fn agent_read_system(
    state: State<'_, AppState>,
    agent_id: String,
) -> AppResult<String> {
    let dir = state.agents_dir().clone();
    run_blocking(move || read_system_prompt(&dir, &agent_id)).await
}

#[tauri::command]
pub async fn agent_list_sessions(
    state: State<'_, AppState>,
    agent_id: String,
) -> AppResult<Vec<AgentSessionMeta>> {
    let dir = state.agents_dir().clone();
    run_blocking(move || list_sessions(&dir, &agent_id)).await
}

#[tauri::command]
pub async fn agent_create_session(
    state: State<'_, AppState>,
    agent_id: String,
    request: NewAgentSession,
) -> AppResult<AgentSessionMeta> {
    let dir = state.agents_dir().clone();
    run_blocking(move || create_session(&dir, &agent_id, request)).await
}

#[tauri::command]
pub async fn agent_load_session(
    state: State<'_, AppState>,
    agent_id: String,
    session_id: String,
) -> AppResult<AgentSessionDetail> {
    let dir = state.agents_dir().clone();
    run_blocking(move || load_session(&dir, &agent_id, &session_id)).await
}

#[tauri::command]
pub async fn agent_append_transcript(
    state: State<'_, AppState>,
    agent_id: String,
    session_id: String,
    entry: TranscriptEntry,
) -> AppResult<TranscriptEntry> {
    let dir = state.agents_dir().clone();
    run_blocking(move || append_transcript(&dir, &agent_id, &session_id, entry)).await
}

#[tauri::command]
pub async fn agent_save_checkpoint(
    state: State<'_, AppState>,
    agent_id: String,
    session_id: String,
    checkpoint: Value,
    status: Option<String>,
) -> AppResult<()> {
    let dir = state.agents_dir().clone();
    run_blocking(move || save_checkpoint(&dir, &agent_id, &session_id, checkpoint, status)).await
}

/// Built-in agents call Skill-compatible tools without requiring MCP to be enabled.
#[tauri::command]
pub async fn agent_tool_call(
    state: State<'_, AppState>,
    name: String,
    args: Value,
) -> AppResult<AgentToolResult> {
    match catalog::invoke(&state, &name, &args).await {
        Ok(result) => Ok(AgentToolResult {
            ok: true,
            result: Some(result),
            error: None,
        }),
        Err(ToolError::Unknown) => Err(AppError::msg("unknown tool")),
        Err(ToolError::BadRequest(message)) | Err(ToolError::Failed(message)) => {
            Ok(AgentToolResult {
                ok: false,
                result: None,
                error: Some(message),
            })
        }
    }
}

#[tauri::command]
pub async fn agent_start_run(
    runner: State<'_, Arc<AgentRunner>>,
    request: AgentStartRunRequest,
) -> AppResult<AgentStartRunResponse> {
    let run_id = runner.start_run(request).await?;
    Ok(AgentStartRunResponse { run_id })
}

#[tauri::command]
pub async fn agent_cancel_run(
    runner: State<'_, Arc<AgentRunner>>,
    run_id: String,
) -> AppResult<()> {
    runner.cancel_run(&run_id).await
}

#[tauri::command]
pub async fn agent_runner_status(
    runner: State<'_, Arc<AgentRunner>>,
) -> AppResult<AgentRunnerStatus> {
    Ok(runner.status().await)
}
