use crate::error::AppResult;
use crate::llm::jobs::LlmJobStatus;
use crate::llm::{complete, jobs, LlmCompleteRequest};
use crate::runtime::run_worker;
use serde_json::Value;

/// Call the configured model from the async worker pool.
///
/// The webview cannot reliably reach provider APIs: WebKit reports a failed
/// cross-origin fetch as `TypeError: Load failed` and the SDK turns that into
/// `Connection error`.
#[tauri::command]
pub async fn complete_llm(request: LlmCompleteRequest) -> AppResult<Value> {
    run_worker(async move { complete(request).await }).await
}

/// Start a model call and return before it finishes.
#[tauri::command]
pub fn start_llm(request: LlmCompleteRequest) -> AppResult<String> {
    jobs::start(request)
}

/// Read a model call started by [`start_llm`].
#[tauri::command]
pub fn poll_llm(id: String) -> AppResult<LlmJobStatus> {
    jobs::poll(&id)
}
