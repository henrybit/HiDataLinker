use crate::error::{AppError, AppResult};
use chrono::SecondsFormat;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

const INDEX_FILE: &str = "index.json";
const MAX_HISTORY: usize = 40;
const MAX_RECORD_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisHistorySummary {
    pub id: String,
    pub created_at: String,
    pub title: String,
    pub object_count: u32,
    pub edge_count: u32,
    pub scopes: Vec<String>,
    pub provider_name: String,
    pub model: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisHistoryRecord {
    #[serde(flatten)]
    pub summary: AnalysisHistorySummary,
    pub graph: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewAnalysisHistory {
    pub title: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub provider_name: String,
    #[serde(default)]
    pub model: String,
    pub graph: Value,
}

pub fn save_history(dir: &Path, request: NewAnalysisHistory) -> AppResult<AnalysisHistorySummary> {
    if !request.graph.is_object() {
        return Err(AppError::msg("analysis history graph must be an object"));
    }
    std::fs::create_dir_all(dir)?;
    let summary = AnalysisHistorySummary {
        id: uuid::Uuid::new_v4().to_string(),
        created_at: chrono::Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        title: clip(&request.title, 200),
        object_count: array_len(&request.graph, "nodes"),
        edge_count: array_len(&request.graph, "edges"),
        scopes: request
            .scopes
            .iter()
            .map(|scope| clip(scope, 160))
            .filter(|scope| !scope.is_empty())
            .take(32)
            .collect(),
        provider_name: clip(&request.provider_name, 120),
        model: clip(&request.model, 120),
    };
    if summary.title.is_empty() {
        return Err(AppError::msg("analysis history title is empty"));
    }
    let record = AnalysisHistoryRecord {
        summary: summary.clone(),
        graph: request.graph,
    };
    let bytes = serde_json::to_vec(&record)?;
    if bytes.len() > MAX_RECORD_BYTES {
        return Err(AppError::msg("analysis history is too large"));
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

pub fn list_history(dir: &Path) -> AppResult<Vec<AnalysisHistorySummary>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    read_index(dir)
}

pub fn read_history(dir: &Path, id: &str) -> AppResult<AnalysisHistoryRecord> {
    let path = record_path(dir, id)?;
    let bytes = std::fs::read(&path)?;
    if bytes.len() > MAX_RECORD_BYTES {
        return Err(AppError::msg("analysis history is too large"));
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

fn record_path(dir: &Path, id: &str) -> AppResult<PathBuf> {
    if uuid::Uuid::parse_str(id).is_err() {
        return Err(AppError::msg("invalid analysis history id"));
    }
    Ok(dir.join(format!("{id}.json")))
}

fn read_index(dir: &Path) -> AppResult<Vec<AnalysisHistorySummary>> {
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

fn write_index(dir: &Path, index: &[AnalysisHistorySummary]) -> AppResult<()> {
    std::fs::create_dir_all(dir)?;
    let json = serde_json::to_string_pretty(index)?;
    std::fs::write(dir.join(INDEX_FILE), json)?;
    Ok(())
}

fn array_len(value: &Value, key: &str) -> u32 {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| items.len().min(u32::MAX as usize) as u32)
        .unwrap_or(0)
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
    use serde_json::json;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("analysis-history-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample(title: &str) -> NewAnalysisHistory {
        NewAnalysisHistory {
            title: title.to_string(),
            scopes: vec!["App / shop".to_string()],
            provider_name: "Work".to_string(),
            model: "gpt-4o-mini".to_string(),
            graph: json!({
                "nodes": [{"id": "orders"}],
                "edges": [{"id": "fk"}],
                "warnings": []
            }),
        }
    }

    #[test]
    fn saves_lists_reads_and_deletes() {
        let dir = temp_dir();
        let saved = save_history(&dir, sample("App / shop")).unwrap();
        assert_eq!(saved.object_count, 1);
        assert_eq!(saved.edge_count, 1);
        assert_eq!(saved.scopes, vec!["App / shop".to_string()]);

        let listed = list_history(&dir).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, saved.id);

        let loaded = read_history(&dir, &saved.id).unwrap();
        assert_eq!(loaded.summary.title, "App / shop");
        assert_eq!(loaded.graph["nodes"][0]["id"], "orders");

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
            let saved = save_history(&dir, sample(&format!("run {index}"))).unwrap();
            newest = saved.id;
        }
        let listed = list_history(&dir).unwrap();
        assert_eq!(listed.len(), MAX_HISTORY);
        assert_eq!(listed[0].id, newest);
        assert_eq!(listed[0].title, format!("run {}", MAX_HISTORY + 1));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn rejects_ids_that_escape_the_directory() {
        let dir = temp_dir();
        assert!(read_history(&dir, "../secret").is_err());
        assert!(delete_history(&dir, "").is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
