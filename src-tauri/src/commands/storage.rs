use crate::error::{AppError, AppResult};
use crate::runtime::run_blocking;
use crate::state::AppState;
use tauri::State;

const MAX_CATALOG_BYTES: usize = 1024 * 1024;

#[tauri::command]
pub async fn read_llm_catalog(state: State<'_, AppState>) -> AppResult<String> {
    let path = state.llm_catalog_path().clone();
    run_blocking(move || read_optional(&path)).await
}

#[tauri::command]
pub async fn write_llm_catalog(state: State<'_, AppState>, contents: String) -> AppResult<()> {
    if contents.len() > MAX_CATALOG_BYTES {
        return Err(AppError::msg("model provider catalog is too large"));
    }
    let path = state.llm_catalog_path().clone();
    run_blocking(move || write_new(&path, &contents)).await
}

#[tauri::command]
pub async fn read_locale(state: State<'_, AppState>) -> AppResult<String> {
    let path = state.locale_path().clone();
    run_blocking(move || read_optional(&path)).await
}

#[tauri::command]
pub async fn write_locale(state: State<'_, AppState>, locale: String) -> AppResult<()> {
    if locale != "en" && locale != "zh" {
        return Err(AppError::msg("unsupported locale"));
    }
    let path = state.locale_path().clone();
    run_blocking(move || write_new(&path, &locale)).await
}

fn read_optional(path: &std::path::Path) -> AppResult<String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error.into()),
    }
}

fn write_new(path: &std::path::Path, contents: &str) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, contents)?;
    Ok(())
}
