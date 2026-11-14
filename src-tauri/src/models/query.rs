use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnMeta {
    pub name: String,
    pub type_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum QueryLogLevel {
    Info,
    Success,
    Notice,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryLogEntry {
    pub level: QueryLogLevel,
    pub text: String,
}

impl QueryLogEntry {
    pub fn info(text: impl Into<String>) -> Self {
        Self {
            level: QueryLogLevel::Info,
            text: text.into(),
        }
    }

    pub fn success(text: impl Into<String>) -> Self {
        Self {
            level: QueryLogLevel::Success,
            text: text.into(),
        }
    }

    pub fn notice(text: impl Into<String>) -> Self {
        Self {
            level: QueryLogLevel::Notice,
            text: text.into(),
        }
    }

    pub fn warning(text: impl Into<String>) -> Self {
        Self {
            level: QueryLogLevel::Warning,
            text: text.into(),
        }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self {
            level: QueryLogLevel::Error,
            text: text.into(),
        }
    }
}

/// A PostgreSQL `EXPLAIN (FORMAT JSON)` document plus how it was produced.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplainPlan {
    pub plan: String,
    pub analyzed: bool,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    pub columns: Vec<ColumnMeta>,
    pub rows: Vec<Vec<Option<String>>>,
    pub affected_rows: u64,
    pub last_insert_id: Option<u64>,
    pub duration_ms: u64,
    pub truncated: bool,
    pub statement_kind: String,
    #[serde(default)]
    pub messages: Vec<QueryLogEntry>,
}
