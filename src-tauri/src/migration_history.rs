use crate::error::{AppError, AppResult};
use crate::models::MigrateProgressEvent;
use chrono::SecondsFormat;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const INDEX_FILE: &str = "index.json";
const MAX_HISTORY: usize = 40;
const MAX_RECORD_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationHistorySummary {
    pub id: String,
    pub created_at: String,
    pub title: String,
    pub status: String,
    pub source_connection: String,
    pub source_name: String,
    pub target_connection: String,
    pub target_name: String,
    pub engine: String,
    pub include_data: bool,
    pub statement_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationHistoryRecord {
    #[serde(flatten)]
    pub summary: MigrationHistorySummary,
    #[serde(default)]
    pub logs: Vec<MigrateProgressEvent>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewMigrationHistory {
    pub title: String,
    pub status: String,
    #[serde(default)]
    pub source_connection: String,
    #[serde(default)]
    pub source_name: String,
    #[serde(default)]
    pub target_connection: String,
    #[serde(default)]
    pub target_name: String,
    #[serde(default)]
    pub engine: String,
    #[serde(default)]
    pub include_data: bool,
    #[serde(default)]
    pub statement_count: u32,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub logs: Vec<MigrateProgressEvent>,
}

pub fn save_history(dir: &Path, request: NewMigrationHistory) -> AppResult<MigrationHistorySummary> {
    let status = normalize_status(&request.status)?;
    std::fs::create_dir_all(dir)?;
    let summary = MigrationHistorySummary {
        id: uuid::Uuid::new_v4().to_string(),
        created_at: chrono::Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        title: clip(&request.title, 200),
        status,
        source_connection: clip(&request.source_connection, 160),
        source_name: clip(&request.source_name, 160),
        target_connection: clip(&request.target_connection, 160),
        target_name: clip(&request.target_name, 160),
        engine: clip(&request.engine, 40),
        include_data: request.include_data,
        statement_count: request.statement_count,
        error: request
            .error
            .as_deref()
            .map(|value| clip(value, 2000))
            .filter(|value| !value.is_empty()),
    };
    if summary.title.is_empty() {
        return Err(AppError::msg("migration history title is empty"));
    }
    let logs = request.logs.into_iter().take(2000).collect::<Vec<_>>();
    let record = MigrationHistoryRecord {
        summary: summary.clone(),
        logs,
    };
    let bytes = serde_json::to_vec(&record)?;
    if bytes.len() > MAX_RECORD_BYTES {
        return Err(AppError::msg("migration history is too large"));
    }
    let path = record_path(dir, &summary.id)?;
    std::fs::write(&path, bytes)?;

    let mut index = read_index(dir).unwrap_or_default();
    index.retain(|item| item.id != summary.id);
    index.insert(0, summary.clone());
    while index.len() > MAX_HISTORY {
        if let Some(removed) = index.pop() {
            let _ = std::fs::remove_file(record_path(dir, &removed.id)?);
        }
    }
    if let Err(error) = write_index(dir, &index) {
        let _ = std::fs::remove_file(path);
        return Err(error);
    }
    Ok(summary)
}

pub fn list_history(dir: &Path) -> AppResult<Vec<MigrationHistorySummary>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    read_index(dir)
}

pub fn read_history(dir: &Path, id: &str) -> AppResult<MigrationHistoryRecord> {
    let path = record_path(dir, id)?;
    let bytes = std::fs::read(&path)?;
    if bytes.len() > MAX_RECORD_BYTES {
        return Err(AppError::msg("migration history is too large"));
    }
    Ok(serde_json::from_slice(&bytes)?)
}

pub fn delete_history(dir: &Path, id: &str) -> AppResult<()> {
    let path = record_path(dir, id)?;
    if dir.exists() {
        let mut index = read_index(dir).unwrap_or_default();
        index.retain(|item| item.id != id);
        write_index(dir, &index)?;
    }
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn normalize_status(status: &str) -> AppResult<String> {
    match status.trim() {
        "success" | "failed" => Ok(status.trim().to_string()),
        _ => Err(AppError::msg("migration history status must be success or failed")),
    }
}

fn record_path(dir: &Path, id: &str) -> AppResult<PathBuf> {
    if uuid::Uuid::parse_str(id).is_err() {
        return Err(AppError::msg("invalid migration history id"));
    }
    Ok(dir.join(format!("{id}.json")))
}

fn read_index(dir: &Path) -> AppResult<Vec<MigrationHistorySummary>> {
    let path = dir.join(INDEX_FILE);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = std::fs::read_to_string(path)?;
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }
    Ok(serde_json::from_str(&raw)?)
}

fn write_index(dir: &Path, index: &[MigrationHistorySummary]) -> AppResult<()> {
    std::fs::create_dir_all(dir)?;
    let json = serde_json::to_string_pretty(index)?;
    std::fs::write(dir.join(INDEX_FILE), json)?;
    Ok(())
}

fn clip(value: &str, max: usize) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= max {
        return trimmed.to_string();
    }
    trimmed.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("migration-history-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample(title: &str, status: &str) -> NewMigrationHistory {
        NewMigrationHistory {
            title: title.to_string(),
            status: status.to_string(),
            source_connection: "Local MySQL".to_string(),
            source_name: "shop".to_string(),
            target_connection: "Remote MySQL".to_string(),
            target_name: "shop_copy".to_string(),
            engine: "mysql".to_string(),
            include_data: true,
            statement_count: 12,
            error: None,
            logs: vec![MigrateProgressEvent {
                phase: "done".into(),
                level: "success".into(),
                object_kind: None,
                object_name: None,
                current: 1,
                total: 1,
                message: "ok".into(),
            }],
        }
    }

    #[test]
    fn saves_lists_reads_and_deletes() {
        let dir = temp_dir();
        let saved = save_history(&dir, sample("Local → Remote", "success")).unwrap();
        assert_eq!(saved.statement_count, 12);
        assert_eq!(saved.status, "success");
        assert_eq!(saved.source_name, "shop");

        let listed = list_history(&dir).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, saved.id);

        let loaded = read_history(&dir, &saved.id).unwrap();
        assert_eq!(loaded.summary.title, "Local → Remote");
        assert_eq!(loaded.logs.len(), 1);

        delete_history(&dir, &saved.id).unwrap();
        assert!(list_history(&dir).unwrap().is_empty());
        assert!(read_history(&dir, &saved.id).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn keeps_the_newest_records_only() {
        let dir = temp_dir();
        let mut newest = String::new();
        for index in 0..(MAX_HISTORY + 2) {
            let saved = save_history(&dir, sample(&format!("run {index}"), "success")).unwrap();
            newest = saved.id;
        }
        let listed = list_history(&dir).unwrap();
        assert_eq!(listed.len(), MAX_HISTORY);
        assert_eq!(listed[0].id, newest);
        assert_eq!(listed[0].title, format!("run {}", MAX_HISTORY + 1));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn rejects_invalid_status_and_ids() {
        let dir = temp_dir();
        assert!(save_history(&dir, sample("bad", "running")).is_err());
        assert!(read_history(&dir, "../secret").is_err());
        assert!(delete_history(&dir, "").is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
