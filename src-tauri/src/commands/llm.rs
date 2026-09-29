use crate::error::AppResult;
use crate::llm::{complete, LlmCompleteRequest};
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
