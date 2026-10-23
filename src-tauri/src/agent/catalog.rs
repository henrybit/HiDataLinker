use crate::commands::with_engine;
use crate::db::ident::validate_ident;
use crate::db::DatabaseEngine;
use crate::error::{AppError, AppResult};
use crate::models::ObjectKind;
use crate::state::AppState;
use serde::Serialize;
use serde_json::{json, Value};

use super::policy;

#[derive(Debug)]
pub enum ToolError {
    Unknown,
    BadRequest(String),
    Failed(String),
}

impl ToolError {
    pub fn message(&self) -> String {
        match self {
            Self::Unknown => "unknown tool".into(),
            Self::BadRequest(message) | Self::Failed(message) => message.clone(),
        }
    }
}

pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub schema: Value,
}

pub fn tool_specs() -> Vec<ToolSpec> {
    let connection = json!({
        "type": "object",
        "properties": {
            "connectionId": { "type": "string", "description": "Saved HiDataLinker connection id" }
        },
        "required": ["connectionId"]
    });
    let schema_object = json!({
        "type": "object",
        "properties": {
            "connectionId": { "type": "string" },
            "schema": { "type": "string", "description": "Database or schema name" }
        },
        "required": ["connectionId", "schema"]
    });
    vec![
        ToolSpec {
            name: "list_connections",
            description: "List saved HiDataLinker connections without passwords, usernames, or certificate paths. Each item has id, name, engine, host, port, and connected.",
            schema: json!({ "type": "object", "properties": {} }),
        },
        ToolSpec {
            name: "connect",
            description: "Open a saved connection by id. Uses the password stored in HiDataLinker. Do not pass a password. Fails when the desktop app has no saved password for that connection.",
            schema: connection.clone(),
        },
        ToolSpec {
            name: "list_databases",
            description: "List databases or schemas on an open connection.",
            schema: connection.clone(),
        },
        ToolSpec {
            name: "list_tables",
            description: "List tables in a database or schema.",
            schema: schema_object.clone(),
        },
        ToolSpec {
            name: "list_views",
            description: "List views in a database or schema.",
            schema: schema_object.clone(),
        },
        ToolSpec {
            name: "list_indexes",
            description: "List indexes in a database or schema.",
            schema: schema_object.clone(),
        },
        ToolSpec {
            name: "list_triggers",
            description: "List triggers in a database or schema.",
            schema: schema_object.clone(),
        },
        ToolSpec {
            name: "list_routines",
            description: "List functions and procedures in a database or schema.",
            schema: schema_object.clone(),
        },
        ToolSpec {
            name: "get_columns",
            description: "List columns for a table or view.",
            schema: json!({
                "type": "object",
                "properties": {
                    "connectionId": { "type": "string" },
                    "schema": { "type": "string" },
                    "table": { "type": "string" }
                },
                "required": ["connectionId", "schema", "table"]
            }),
        },
        ToolSpec {
            name: "get_ddl",
            description: "Return the DDL for one object. kind is table, view, index, trigger, function, or procedure.",
            schema: json!({
                "type": "object",
                "properties": {
                    "connectionId": { "type": "string" },
                    "schema": { "type": "string" },
                    "kind": { "type": "string", "enum": ["table", "view", "index", "trigger", "function", "procedure"] },
                    "name": { "type": "string" }
                },
                "required": ["connectionId", "schema", "kind", "name"]
            }),
        },
        ToolSpec {
            name: "preview_table",
            description: "Read a page of rows from a table. limit is capped at 1000.",
            schema: json!({
                "type": "object",
                "properties": {
                    "connectionId": { "type": "string" },
                    "schema": { "type": "string" },
                    "table": { "type": "string" },
                    "limit": { "type": "integer" },
                    "offset": { "type": "integer" }
                },
                "required": ["connectionId", "schema", "table"]
            }),
        },
        ToolSpec {
            name: "query",
            description: "Run one read-only statement: SELECT, WITH … SELECT, EXPLAIN, SHOW, DESCRIBE, TABLE, or VALUES. Writes, multiple statements, and file-reading functions are rejected. Results are capped by the desktop engine (5000 rows).",
            schema: json!({
                "type": "object",
                "properties": {
                    "connectionId": { "type": "string" },
                    "schema": { "type": "string", "description": "Optional database or schema search path" },
                    "sql": { "type": "string" }
                },
                "required": ["connectionId", "sql"]
            }),
        },
    ]
}

pub fn is_tool(name: &str) -> bool {
    tool_specs().iter().any(|tool| tool.name == name)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentConnection {
    id: String,
    name: String,
    engine: String,
    host: String,
    port: u16,
    connected: bool,
}

pub async fn invoke(state: &AppState, name: &str, args: &Value) -> Result<Value, ToolError> {
    if !is_tool(name) {
        return Err(ToolError::Unknown);
    }
    match name {
        "list_connections" => list_connections(state).await,
        "connect" => connect(state, args).await,
        "list_databases" => {
            engine_call(state, args, |engine| async move {
                engine.list_databases().await
            })
            .await
        }
        "list_tables" => {
            let schema = schema_name(args)?;
            engine_call(state, args, move |engine| async move {
                engine.list_tables(&schema).await
            })
            .await
        }
        "list_views" => {
            let schema = schema_name(args)?;
            engine_call(state, args, move |engine| async move {
                engine.list_views(&schema).await
            })
            .await
        }
        "list_indexes" => {
            let schema = schema_name(args)?;
            engine_call(state, args, move |engine| async move {
                engine.list_indexes(&schema).await
            })
            .await
        }
        "list_triggers" => {
            let schema = schema_name(args)?;
            engine_call(state, args, move |engine| async move {
                engine.list_triggers(&schema).await
            })
            .await
        }
        "list_routines" => {
            let schema = schema_name(args)?;
            engine_call(state, args, move |engine| async move {
                engine.list_routines(&schema).await
            })
            .await
        }
        "get_columns" => {
            let schema = schema_name(args)?;
            let table = ident_arg(args, "table")?;
            engine_call(state, args, move |engine| async move {
                engine.get_columns(&schema, &table).await
            })
            .await
        }
        "get_ddl" => {
            let schema = schema_name(args)?;
            let name = ident_arg(args, "name")?;
            let kind = object_kind(args)?;
            engine_call(state, args, move |engine| async move {
                engine.get_ddl(&schema, kind, &name).await
            })
            .await
        }
        "preview_table" => preview_table(state, args).await,
        "query" => query(state, args).await,
        _ => Err(ToolError::Unknown),
    }
}

async fn list_connections(state: &AppState) -> Result<Value, ToolError> {
    let profiles = state.list_profiles().await;
    let items: Vec<AgentConnection> = profiles
        .into_iter()
        .map(|profile| AgentConnection {
            connected: state.is_connected(&profile.id),
            id: profile.id,
            name: profile.name,
            engine: profile.engine,
            host: profile.host,
            port: profile.port,
        })
        .collect();
    serde_json::to_value(items).map_err(|error| ToolError::Failed(error.to_string()))
}

async fn connect(state: &AppState, args: &Value) -> Result<Value, ToolError> {
    let id = connection_id(args)?;
    let profile = state.get_profile(&id).await.map_err(tool_failed)?;
    if profile.password.as_deref().unwrap_or("").is_empty() {
        return Err(ToolError::BadRequest(
            "this connection has no saved password; connect it once in HiDataLinker before an agent can use it".into(),
        ));
    }
    let result = state.connect(&id, None).await.map_err(tool_failed)?;
    Ok(json!({
        "connected": true,
        "evictedIds": result.evicted_ids,
    }))
}

async fn preview_table(state: &AppState, args: &Value) -> Result<Value, ToolError> {
    let schema = schema_name(args)?;
    let table = ident_arg(args, "table")?;
    let limit = match args.get("limit") {
        None | Some(Value::Null) => 100,
        Some(value) => u32_arg(value, "limit")?,
    };
    let limit = policy::clamp_preview_limit(limit);
    let offset = match args.get("offset") {
        None | Some(Value::Null) => 0,
        Some(value) => u64_arg(value, "offset")?,
    };
    engine_call(state, args, move |engine| async move {
        engine.preview_table(&schema, &table, limit, offset).await
    })
    .await
}

async fn query(state: &AppState, args: &Value) -> Result<Value, ToolError> {
    let sql = required_str(args, "sql")?;
    let sql = policy::assert_read_only_sql(&sql).map_err(ToolError::BadRequest)?;
    let schema = optional_str(args, "schema")?;
    if let Some(schema) = &schema {
        validate_ident(schema).map_err(|error| ToolError::BadRequest(error.user_message()))?;
    }
    engine_call(state, args, move |engine| async move {
        engine.execute_sql(schema.as_deref(), &sql).await
    })
    .await
}

async fn engine_call<T, F, Fut>(state: &AppState, args: &Value, work: F) -> Result<Value, ToolError>
where
    T: Serialize + Send + 'static,
    F: FnOnce(crate::db::LiveEngine) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = AppResult<T>> + Send + 'static,
{
    let connection_id = connection_id(args)?;
    let value = with_engine(state, connection_id, work)
        .await
        .map_err(tool_failed)?;
    serde_json::to_value(value).map_err(|error| ToolError::Failed(error.to_string()))
}

fn connection_id(args: &Value) -> Result<String, ToolError> {
    let id = required_str(args, "connectionId")?;
    validate_ident(&id).map_err(|error| ToolError::BadRequest(error.user_message()))?;
    Ok(id)
}

fn schema_name(args: &Value) -> Result<String, ToolError> {
    ident_arg(args, "schema")
}

fn ident_arg(args: &Value, key: &str) -> Result<String, ToolError> {
    let value = required_str(args, key)?;
    validate_ident(&value).map_err(|error| ToolError::BadRequest(error.user_message()))?;
    Ok(value)
}

fn object_kind(args: &Value) -> Result<ObjectKind, ToolError> {
    let raw = args
        .get("kind")
        .cloned()
        .ok_or_else(|| ToolError::BadRequest("kind is required".into()))?;
    serde_json::from_value(raw).map_err(|_| {
        ToolError::BadRequest(
            "kind must be table, view, index, trigger, function, or procedure".into(),
        )
    })
}

fn required_str(args: &Value, key: &str) -> Result<String, ToolError> {
    match args.get(key) {
        Some(Value::String(value)) if !value.trim().is_empty() => Ok(value.trim().to_string()),
        _ => Err(ToolError::BadRequest(format!("{key} is required"))),
    }
}

fn optional_str(args: &Value, key: &str) -> Result<Option<String>, ToolError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if value.trim().is_empty() => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.trim().to_string())),
        _ => Err(ToolError::BadRequest(format!("{key} must be a string"))),
    }
}

fn u32_arg(value: &Value, key: &str) -> Result<u32, ToolError> {
    value
        .as_u64()
        .and_then(|number| u32::try_from(number).ok())
        .ok_or_else(|| ToolError::BadRequest(format!("{key} must be a positive integer")))
}

fn u64_arg(value: &Value, key: &str) -> Result<u64, ToolError> {
    value
        .as_u64()
        .ok_or_else(|| ToolError::BadRequest(format!("{key} must be a positive integer")))
}

fn tool_failed(error: AppError) -> ToolError {
    let message = error.user_message();
    if matches!(
        error,
        AppError::NotConnected(_) | AppError::ProfileNotFound(_)
    ) {
        ToolError::BadRequest(message)
    } else {
        ToolError::Failed(message)
    }
}

pub fn audit_fields(tool: &str, args: &Value) -> (Option<String>, String) {
    let connection_id = args
        .get("connectionId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let summary = match tool {
        "query" => flatten(args.get("sql").and_then(Value::as_str).unwrap_or("")),
        "preview_table" | "get_columns" => join_names(args, &["schema", "table"]),
        "get_ddl" => join_names(args, &["schema", "kind", "name"]),
        "list_tables" | "list_views" | "list_indexes" | "list_triggers" | "list_routines" => {
            join_names(args, &["schema"])
        }
        "connect" | "list_databases" => connection_id.clone().unwrap_or_default(),
        _ => String::new(),
    };
    (connection_id, truncate(&summary, 160))
}

fn join_names(args: &Value, keys: &[&str]) -> String {
    keys.iter()
        .filter_map(|key| args.get(*key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(".")
}

fn flatten(sql: &str) -> String {
    sql.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate(text: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (index, ch) in text.chars().enumerate() {
        if index >= max_chars {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ConnectionProfile;
    use crate::paths::AppPaths;

    fn profile(password: Option<&str>) -> ConnectionProfile {
        ConnectionProfile {
            id: "conn-1".into(),
            name: "Orders".into(),
            engine: "mysql".into(),
            host: "127.0.0.1".into(),
            port: 3306,
            username: "root".into(),
            password: password.map(str::to_string),
            database: Some("app".into()),
            ssl_ca: Some("/tmp/ca.pem".into()),
            ssl_cert: None,
            ssl_key: Some("/tmp/key.pem".into()),
            ssl_verify: false,
            oracle_version: None,
            oracle_connect: None,
            oracle_instant_client: None,
            save_password: password.is_some(),
        }
    }

    async fn loaded() -> AppState {
        let root =
            std::env::temp_dir().join(format!("hidatalinker-agent-{}", uuid::Uuid::new_v4()));
        let paths = AppPaths::from_root(root);
        AppState::load(paths, None).unwrap()
    }

    #[tokio::test]
    async fn list_connections_omits_secrets() {
        let state = loaded().await;
        state
            .upsert_profile(profile(Some("s3cret-password")))
            .await
            .unwrap();
        let value = invoke(&state, "list_connections", &json!({}))
            .await
            .unwrap();
        let text = value.to_string();
        assert!(!text.contains("s3cret-password"));
        assert!(!text.contains("username"));
        assert!(!text.contains("sslKey"));
        assert!(!text.contains("/tmp/ca.pem"));
        assert_eq!(value[0]["id"], "conn-1");
        assert_eq!(value[0]["name"], "Orders");
        assert_eq!(value[0]["engine"], "mysql");
        assert_eq!(value[0]["host"], "127.0.0.1");
        assert_eq!(value[0]["port"], 3306);
        assert_eq!(value[0]["connected"], false);
        let _ = std::fs::remove_dir_all(state_root(&state));
    }

    #[tokio::test]
    async fn query_rejects_a_delete_before_connecting() {
        let state = loaded().await;
        let error = invoke(
            &state,
            "query",
            &json!({"connectionId": "conn-1", "sql": "DELETE FROM orders"}),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, ToolError::BadRequest(_)));
        assert!(error.message().contains("DELETE"));
        let _ = std::fs::remove_dir_all(state_root(&state));
    }

    #[tokio::test]
    async fn connect_rejects_a_missing_password() {
        let state = loaded().await;
        state.upsert_profile(profile(None)).await.unwrap();
        let error = invoke(&state, "connect", &json!({"connectionId": "conn-1"}))
            .await
            .unwrap_err();
        assert!(matches!(error, ToolError::BadRequest(_)));
        assert!(error.message().contains("saved password"));
        let _ = std::fs::remove_dir_all(state_root(&state));
    }

    fn state_root(state: &AppState) -> std::path::PathBuf {
        state
            .locale_path()
            .parent()
            .and_then(|path| path.parent())
            .unwrap()
            .to_path_buf()
    }
}
