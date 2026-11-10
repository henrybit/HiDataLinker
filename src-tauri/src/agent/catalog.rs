use crate::db::ident::validate_ident;
use crate::db::DatabaseEngine;
use crate::error::{AppError, AppResult};
use crate::models::ObjectKind;
use crate::runtime::run_db;
use crate::sql_audit::AuditSource;
use crate::state::AppState;
use serde::Serialize;
use serde_json::{json, Value};

use super::{policy, script};

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
            description: "Run one SQL statement. In read-only mode only SELECT/WITH…SELECT/EXPLAIN/SHOW/DESCRIBE/TABLE/VALUES are allowed. In full authorization mode DML/DDL are allowed. Multiple statements and file/OS side-channel functions are always rejected. Results are capped by the desktop engine (5000 rows).",
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
        ToolSpec {
            name: "write_script",
            description: "Write a Python (.py), Node (.js/.mjs/.cjs), or shell (.sh) script into ~/.hidatalinker/temp/scripts/<runId>/. Requires full authorization. Use before run_script for import/ETL automation.",
            schema: json!({
                "type": "object",
                "properties": {
                    "filename": { "type": "string", "description": "Plain filename with .py, .js/.mjs/.cjs, or .sh extension" },
                    "content": { "type": "string", "description": "Full script source" },
                    "overwrite": { "type": "boolean", "description": "Replace an existing file (default false)" }
                },
                "required": ["filename", "content"]
            }),
        },
        ToolSpec {
            name: "run_script",
            description: "Execute a previously written script from the run's temp workspace with python3, node, or bash/sh. Requires full authorization. stdout/stderr are truncated.",
            schema: json!({
                "type": "object",
                "properties": {
                    "filename": { "type": "string" },
                    "args": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional argv passed after the script path"
                    },
                    "timeoutMs": { "type": "integer", "description": "Timeout in ms (default 60000, max 120000)" }
                },
                "required": ["filename"]
            }),
        },
        ToolSpec {
            name: "list_scripts",
            description: "List scripts written for the current agent run under ~/.hidatalinker/temp/scripts/<runId>/. Requires full authorization.",
            schema: json!({ "type": "object", "properties": {} }),
        },
    ]
}

pub fn is_tool(name: &str) -> bool {
    tool_specs().iter().any(|tool| tool.name == name)
}

/// Tools that must not be exposed over MCP/HTTP (desktop agent run only).
pub fn is_desktop_only_tool(name: &str) -> bool {
    script::is_script_tool(name)
}

#[derive(Clone, Debug)]
pub struct ToolContext {
    pub auth_mode: policy::SqlAuthMode,
    pub run_id: Option<String>,
}

impl Default for ToolContext {
    fn default() -> Self {
        Self {
            auth_mode: policy::SqlAuthMode::ReadOnly,
            run_id: None,
        }
    }
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
    invoke_with_context(state, name, args, ToolContext::default()).await
}

#[allow(dead_code)]
pub async fn invoke_with_auth(
    state: &AppState,
    name: &str,
    args: &Value,
    auth_mode: policy::SqlAuthMode,
) -> Result<Value, ToolError> {
    invoke_with_context(
        state,
        name,
        args,
        ToolContext {
            auth_mode,
            run_id: None,
        },
    )
    .await
}

pub async fn invoke_with_context(
    state: &AppState,
    name: &str,
    args: &Value,
    ctx: ToolContext,
) -> Result<Value, ToolError> {
    if !is_tool(name) {
        return Err(ToolError::Unknown);
    }
    match name {
        "list_connections" => list_connections(state).await,
        "connect" => connect(state, args).await,
        "list_databases" => {
            engine_call(state, &ctx, args, |engine| async move {
                engine.list_databases().await
            })
            .await
        }
        "list_tables" => {
            let schema = schema_name(args)?;
            engine_call(state, &ctx, args, move |engine| async move {
                engine.list_tables(&schema).await
            })
            .await
        }
        "list_views" => {
            let schema = schema_name(args)?;
            engine_call(state, &ctx, args, move |engine| async move {
                engine.list_views(&schema).await
            })
            .await
        }
        "list_indexes" => {
            let schema = schema_name(args)?;
            engine_call(state, &ctx, args, move |engine| async move {
                engine.list_indexes(&schema).await
            })
            .await
        }
        "list_triggers" => {
            let schema = schema_name(args)?;
            engine_call(state, &ctx, args, move |engine| async move {
                engine.list_triggers(&schema).await
            })
            .await
        }
        "list_routines" => {
            let schema = schema_name(args)?;
            engine_call(state, &ctx, args, move |engine| async move {
                engine.list_routines(&schema).await
            })
            .await
        }
        "get_columns" => {
            let schema = schema_name(args)?;
            let table = ident_arg(args, "table")?;
            engine_call(state, &ctx, args, move |engine| async move {
                engine.get_columns(&schema, &table).await
            })
            .await
        }
        "get_ddl" => {
            let schema = schema_name(args)?;
            let name = ident_arg(args, "name")?;
            let kind = object_kind(args)?;
            engine_call(state, &ctx, args, move |engine| async move {
                engine.get_ddl(&schema, kind, &name).await
            })
            .await
        }
        "preview_table" => preview_table(state, &ctx, args).await,
        "query" => query(state, &ctx, args).await,
        "write_script" => write_script_tool(state, args, &ctx),
        "run_script" => run_script_tool(state, args, &ctx).await,
        "list_scripts" => list_scripts_tool(state, &ctx),
        _ => Err(ToolError::Unknown),
    }
}

fn script_gate(ctx: &ToolContext) -> Result<String, ToolError> {
    script::require_full_auth(ctx.auth_mode).map_err(ToolError::BadRequest)?;
    script::require_run_id(ctx.run_id.as_deref()).map_err(ToolError::BadRequest)
}

fn write_script_tool(
    state: &AppState,
    args: &Value,
    ctx: &ToolContext,
) -> Result<Value, ToolError> {
    let run_id = script_gate(ctx)?;
    let filename = required_str(args, "filename")?;
    let content = required_str_allow_empty(args, "content")?;
    let overwrite = args
        .get("overwrite")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    script::write_script(state.temp_dir(), &run_id, &filename, &content, overwrite).map_err(
        |message| {
            if message.starts_with("failed to") {
                ToolError::Failed(message)
            } else {
                ToolError::BadRequest(message)
            }
        },
    )
}

async fn run_script_tool(
    state: &AppState,
    args: &Value,
    ctx: &ToolContext,
) -> Result<Value, ToolError> {
    let run_id = script_gate(ctx)?;
    let filename = required_str(args, "filename")?;
    let script_args = optional_string_array(args, "args")?;
    let timeout_ms = match args.get("timeoutMs") {
        None | Some(Value::Null) => None,
        Some(value) => Some(u64_arg(value, "timeoutMs")?),
    };
    script::run_script(
        state.temp_dir(),
        &run_id,
        &filename,
        &script_args,
        timeout_ms,
    )
    .await
    .map_err(|message| {
        if message.contains("timed out") || message.contains("not found on PATH") {
            ToolError::Failed(message)
        } else if message.starts_with("script not found")
            || message.contains("must not")
            || message.contains("too long")
            || message.contains("invalid")
            || message.contains("escapes")
            || message.contains("require")
        {
            ToolError::BadRequest(message)
        } else {
            ToolError::Failed(message)
        }
    })
}

fn list_scripts_tool(state: &AppState, ctx: &ToolContext) -> Result<Value, ToolError> {
    let run_id = script_gate(ctx)?;
    script::list_scripts(state.temp_dir(), &run_id).map_err(ToolError::Failed)
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
    let result = ensure_connected(state, &id).await?;
    Ok(json!({
        "connected": true,
        "evictedIds": result.map(|r| r.evicted_ids).unwrap_or_default(),
    }))
}

/// Open a saved connection when the agent needs it (saved password required).
async fn ensure_connected(
    state: &AppState,
    id: &str,
) -> Result<Option<crate::models::ConnectResult>, ToolError> {
    if state.is_connected(id) {
        return Ok(None);
    }
    let profile = state.get_profile(id).await.map_err(tool_failed)?;
    if profile.password.as_deref().unwrap_or("").is_empty() {
        return Err(ToolError::BadRequest(
            "this connection has no saved password; connect it once in HiDataLinker before an agent can use it".into(),
        ));
    }
    let result = state.connect(id, None).await.map_err(tool_failed)?;
    Ok(Some(result))
}

async fn preview_table(
    state: &AppState,
    ctx: &ToolContext,
    args: &Value,
) -> Result<Value, ToolError> {
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
    engine_call(state, &ctx, args, move |engine| async move {
        engine.preview_table(&schema, &table, limit, offset).await
    })
    .await
}

async fn query(state: &AppState, ctx: &ToolContext, args: &Value) -> Result<Value, ToolError> {
    let sql = required_str(args, "sql")?;
    let sql = policy::assert_agent_sql(&sql, ctx.auth_mode).map_err(ToolError::BadRequest)?;
    let schema = optional_str(args, "schema")?;
    if let Some(schema) = &schema {
        validate_ident(schema).map_err(|error| ToolError::BadRequest(error.user_message()))?;
    }
    engine_call(state, &ctx, args, move |engine| async move {
        engine.execute_sql(schema.as_deref(), &sql).await
    })
    .await
}

async fn engine_call<T, F, Fut>(
    state: &AppState,
    ctx: &ToolContext,
    args: &Value,
    work: F,
) -> Result<Value, ToolError>
where
    T: Serialize + Send + 'static,
    F: FnOnce(crate::db::LiveEngine) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = AppResult<T>> + Send + 'static,
{
    let connection_id = connection_id(args)?;
    // Agent tools auto-connect on demand so analysis does not require a prior UI connect.
    ensure_connected(state, &connection_id).await?;
    let detail = match &ctx.run_id {
        Some(run_id) => format!("run:{run_id}"),
        None => "mcp".to_string(),
    };
    let engine = state
        .engine(&connection_id)
        .map_err(tool_failed)?
        .with_source(AuditSource::Agent, Some(detail));
    let value = run_db(work(engine)).await.map_err(tool_failed)?;
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

fn required_str_allow_empty(args: &Value, key: &str) -> Result<String, ToolError> {
    match args.get(key) {
        Some(Value::String(value)) => Ok(value.clone()),
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

fn optional_string_array(args: &Value, key: &str) -> Result<Vec<String>, ToolError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(items)) => {
            let mut out = Vec::with_capacity(items.len());
            for (index, item) in items.iter().enumerate() {
                let Some(text) = item.as_str() else {
                    return Err(ToolError::BadRequest(format!(
                        "{key}[{index}] must be a string"
                    )));
                };
                out.push(text.to_string());
            }
            if out.len() > 32 {
                return Err(ToolError::BadRequest(format!(
                    "{key} supports at most 32 entries"
                )));
            }
            Ok(out)
        }
        _ => Err(ToolError::BadRequest(format!("{key} must be an array of strings"))),
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
        "write_script" | "run_script" => args
            .get("filename")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        "list_scripts" => "list".into(),
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
