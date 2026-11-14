use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseInfo {
    pub name: String,
    pub charset: Option<String>,
    pub collation: Option<String>,
    pub is_system: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CharsetInfo {
    pub name: String,
    pub default_collation: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollationInfo {
    pub name: String,
    pub charset: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CharsetCatalog {
    pub charsets: Vec<CharsetInfo>,
    pub collations: Vec<CollationInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableInfo {
    pub name: String,
    pub engine: Option<String>,
    pub table_rows: Option<u64>,
    pub data_length: Option<u64>,
    pub comment: String,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    /// Parent table when this table is a partition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewInfo {
    pub name: String,
    pub updatable: bool,
    pub check_option: Option<String>,
    pub security_type: Option<String>,
    pub definer: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexInfo {
    pub name: String,
    pub table_name: String,
    pub unique: bool,
    pub primary: bool,
    pub index_type: String,
    pub columns: Vec<String>,
    pub comment: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerInfo {
    pub name: String,
    pub table_name: String,
    pub event: String,
    pub timing: String,
    pub definer: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineInfo {
    pub name: String,
    pub routine_type: String,
    pub returns: Option<String>,
    pub deterministic: bool,
    pub data_access: Option<String>,
    pub security_type: Option<String>,
    pub definer: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub column_type: String,
    pub nullable: bool,
    pub key: String,
    pub default_value: Option<String>,
    pub extra: String,
    pub comment: String,
    pub ordinal: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ObjectKind {
    Table,
    View,
    Index,
    Trigger,
    Function,
    Procedure,
    Sequence,
    Type,
    Extension,
    /// Row-level security policy, named `table.policy`.
    Policy,
}

/// Entry of an engine-specific object folder (PostgreSQL sequences, types, …).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaObjectInfo {
    pub name: String,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum HealthSeverity {
    Error,
    Warning,
    Info,
}

/// One problem found by a schema health rule. `rule` is a stable id the UI
/// localizes; `facts` carries the object-specific details (types, sizes, …).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthFinding {
    pub rule: String,
    pub severity: HealthSeverity,
    pub object: String,
    pub facts: String,
    /// Suggested fix; never executed by the app itself.
    pub fix_sql: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaHealthReport {
    pub findings: Vec<HealthFinding>,
    pub rules_checked: Vec<String>,
    /// When usage statistics were last reset; unused-index findings depend on it.
    pub stats_reset: Option<String>,
    pub duration_ms: u64,
}
