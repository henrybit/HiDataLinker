use super::dump::{build_dump, build_table_data_dump, DumpDialect};
use super::engine::DatabaseEngine;
use super::ident::{
    create_oracle_schema_statements, drop_oracle_schema_sql, oracle_set_schema_sql, qualify_pg,
    quote_ident_pg, validate_ident,
};
use super::sql::{apply_default_query_limit, statement_kind, SqlDialect, DEFAULT_QUERY_ROW_LIMIT};
use crate::error::{AppError, AppResult};
use crate::models::{
    CharsetCatalog, ColumnInfo, ColumnMeta, ConnectionProfile, DatabaseInfo, IndexInfo, ObjectKind,
    QueryLogEntry, QueryResult, RoutineInfo, TableInfo, TestConnectionRequest, TriggerInfo,
    ViewInfo,
};
use async_trait::async_trait;
use oracle_rs::{BindParam, ColumnInfo as OracleColumn, Config, Connection, LobValue, Row, Value};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

const MAX_RESULT_ROWS: usize = 5_000;

const SYSTEM_USERS: &[&str] = &[
    "ANONYMOUS",
    "APPQOSSYS",
    "AUDSYS",
    "CTXSYS",
    "DBSFWUSER",
    "DBSNMP",
    "DIP",
    "DVSYS",
    "GGSYS",
    "GSMADMIN_INTERNAL",
    "GSMCATUSER",
    "GSMROOTUSER",
    "GSMUSER",
    "LBACSYS",
    "MDDATA",
    "MDSYS",
    "OJVMSYS",
    "OLAPSYS",
    "ORACLE_OCM",
    "ORDDATA",
    "ORDPLUGINS",
    "ORDSYS",
    "OUTLN",
    "REMOTE_SCHEDULER_AGENT",
    "SI_INFORMTN_SCHEMA",
    "SYS",
    "SYS$UMF",
    "SYSBACKUP",
    "SYSDG",
    "SYSKM",
    "SYSRAC",
    "SYSTEM",
    "WMSYS",
    "XDB",
    "XS$NULL",
];

#[derive(Clone)]
pub struct OracleEngine {
    config: Config,
    conn: Arc<Mutex<Option<Arc<Connection>>>>,
}

impl OracleEngine {
    pub fn from_profile(profile: &ConnectionProfile) -> AppResult<Self> {
        Ok(Self {
            config: build_config(
                &profile.host,
                profile.port,
                &profile.username,
                profile.password.as_deref(),
                profile.database.as_deref(),
                profile.ssl_ca.as_deref(),
                profile.ssl_verify,
                profile.oracle_version.as_deref(),
                profile.oracle_connect.as_deref(),
            )?,
            conn: Arc::new(Mutex::new(None)),
        })
    }

    pub async fn test(request: &TestConnectionRequest) -> AppResult<()> {
        let engine = Self {
            config: build_config(
                &request.host,
                request.port,
                &request.username,
                request.password.as_deref(),
                request.database.as_deref(),
                request.ssl_ca.as_deref(),
                request.ssl_verify,
                request.oracle_version.as_deref(),
                request.oracle_connect.as_deref(),
            )?,
            conn: Arc::new(Mutex::new(None)),
        };
        engine.ping().await
    }

    async fn connection(&self) -> AppResult<Arc<Connection>> {
        let mut guard = self.conn.lock().await;
        let reconnect = match guard.as_ref() {
            None => true,
            Some(connection) => connection.ping().await.is_err(),
        };
        if reconnect {
            *guard = Some(Arc::new(open_connection(self.config.clone()).await?));
        }
        guard
            .as_ref()
            .cloned()
            .ok_or_else(|| AppError::msg("Oracle connection closed"))
    }

    async fn with_conn<T, F, Fut>(&self, f: F) -> AppResult<T>
    where
        F: FnOnce(Arc<Connection>) -> Fut,
        Fut: std::future::Future<Output = AppResult<T>>,
    {
        let connection = self.connection().await?;
        f(connection).await
    }
}

#[async_trait]
impl DatabaseEngine for OracleEngine {
    async fn ping(&self) -> AppResult<()> {
        self.with_conn(|conn| async move {
            conn.ping().await?;
            Ok(())
        })
        .await
    }

    async fn list_databases(&self) -> AppResult<Vec<DatabaseInfo>> {
        self.with_conn(|conn| async move {
            let result = conn
                .query("SELECT username FROM all_users ORDER BY username", &[])
                .await?;
            let mut databases = Vec::new();
            for row in result.rows {
                let Some(name) = cell_string(&row, 0) else {
                    continue;
                };
                databases.push(DatabaseInfo {
                    is_system: is_oracle_system(&name),
                    name,
                    charset: None,
                    collation: None,
                });
            }
            Ok(databases)
        })
        .await
    }

    async fn create_database(
        &self,
        name: &str,
        _charset: Option<&str>,
        _collation: Option<&str>,
    ) -> AppResult<()> {
        let statements = create_oracle_schema_statements(name)?;
        self.with_conn(move |conn| async move {
            for sql in &statements {
                run_statement(&conn, sql).await?;
            }
            conn.commit().await?;
            Ok(())
        })
        .await
    }

    async fn drop_database(&self, name: &str) -> AppResult<()> {
        let name = name.trim().to_string();
        if is_oracle_system(&name) {
            return Err(AppError::msg(format!("cannot drop system schema: {name}")));
        }
        let sql = drop_oracle_schema_sql(&name)?;
        self.with_conn(move |conn| async move {
            run_statement(&conn, &sql).await?;
            conn.commit().await?;
            Ok(())
        })
        .await
    }

    async fn dump_database(
        &self,
        name: &str,
        include_schema: bool,
        include_data: bool,
    ) -> AppResult<String> {
        let name = name.trim().to_string();
        validate_ident(&name)?;
        build_dump(
            self,
            &name,
            include_schema,
            include_data,
            DumpDialect::Oracle,
        )
        .await
    }

    async fn dump_table(&self, schema: &str, table: &str) -> AppResult<String> {
        let schema = schema.trim().to_string();
        let table = table.trim().to_string();
        validate_ident(&schema)?;
        validate_ident(&table)?;
        build_table_data_dump(self, &schema, &table, DumpDialect::Oracle).await
    }

    async fn list_charset_catalog(&self) -> AppResult<CharsetCatalog> {
        Ok(CharsetCatalog {
            charsets: Vec::new(),
            collations: Vec::new(),
        })
    }

    async fn list_tables(&self, schema: &str) -> AppResult<Vec<TableInfo>> {
        let schema = schema.trim().to_string();
        validate_ident(&schema)?;
        self.with_conn(move |conn| async move {
            let result = conn
                .query(
                    "SELECT t.table_name, t.tablespace_name, t.num_rows, NVL(c.comments, '')
                     FROM all_tables t
                     LEFT JOIN all_tab_comments c
                       ON c.owner = t.owner AND c.table_name = t.table_name AND c.table_type = 'TABLE'
                     WHERE t.owner = :1 AND t.nested = 'NO'
                     ORDER BY t.table_name",
                    &[Value::String(schema)],
                )
                .await?;
            let mut tables = Vec::new();
            for row in result.rows {
                let Some(name) = cell_string(&row, 0) else {
                    continue;
                };
                tables.push(TableInfo {
                    name,
                    engine: cell_string(&row, 1),
                    table_rows: cell_u64(&row, 2),
                    data_length: None,
                    comment: cell_string(&row, 3).unwrap_or_default(),
                    created_at: None,
                    updated_at: None,
                });
            }
            Ok(tables)
        })
        .await
    }

    async fn list_views(&self, schema: &str) -> AppResult<Vec<ViewInfo>> {
        let schema = schema.trim().to_string();
        validate_ident(&schema)?;
        self.with_conn(move |conn| async move {
            let result = conn
                .query(
                    "SELECT view_name FROM all_views WHERE owner = :1 ORDER BY view_name",
                    &[Value::String(schema)],
                )
                .await?;
            Ok(result
                .rows
                .iter()
                .filter_map(|row| {
                    Some(ViewInfo {
                        name: cell_string(row, 0)?,
                        updatable: false,
                        check_option: None,
                        security_type: None,
                        definer: None,
                    })
                })
                .collect())
        })
        .await
    }

    async fn list_indexes(&self, schema: &str) -> AppResult<Vec<IndexInfo>> {
        let schema = schema.trim().to_string();
        validate_ident(&schema)?;
        self.with_conn(move |conn| async move {
            let result = conn
                .query(
                    "SELECT i.index_name, i.table_name, i.uniqueness, i.index_type, ic.column_name,
                            CASE WHEN c.constraint_type = 'P' THEN 1 ELSE 0 END
                     FROM all_indexes i
                     JOIN all_ind_columns ic
                       ON ic.index_owner = i.owner AND ic.index_name = i.index_name
                     LEFT JOIN all_constraints c
                       ON c.owner = i.owner AND c.index_name = i.index_name AND c.constraint_type = 'P'
                     WHERE i.owner = :1
                     ORDER BY i.table_name, i.index_name, ic.column_position",
                    &[Value::String(schema)],
                )
                .await?;
            let mut indexes = Vec::<IndexInfo>::new();
            for row in result.rows {
                let Some(name) = cell_string(&row, 0) else {
                    continue;
                };
                let Some(table_name) = cell_string(&row, 1) else {
                    continue;
                };
                let column = cell_string(&row, 4).unwrap_or_default();
                if let Some(last) = indexes.last_mut() {
                    if last.name == name && last.table_name == table_name {
                        last.columns.push(column);
                        continue;
                    }
                }
                indexes.push(IndexInfo {
                    name,
                    table_name,
                    unique: cell_string(&row, 2).is_some_and(|value| value.eq_ignore_ascii_case("UNIQUE")),
                    primary: cell_u64(&row, 5) == Some(1),
                    index_type: cell_string(&row, 3).unwrap_or_else(|| "INDEX".to_string()),
                    columns: vec![column],
                    comment: String::new(),
                });
            }
            Ok(indexes)
        })
        .await
    }

    async fn list_triggers(&self, schema: &str) -> AppResult<Vec<TriggerInfo>> {
        let schema = schema.trim().to_string();
        validate_ident(&schema)?;
        self.with_conn(move |conn| async move {
            let result = conn
                .query(
                    "SELECT trigger_name, table_name, triggering_event, trigger_type
                     FROM all_triggers WHERE owner = :1 ORDER BY trigger_name",
                    &[Value::String(schema)],
                )
                .await?;
            Ok(result
                .rows
                .iter()
                .filter_map(|row| {
                    Some(TriggerInfo {
                        name: cell_string(row, 0)?,
                        table_name: cell_string(row, 1)?,
                        event: cell_string(row, 2).unwrap_or_default(),
                        timing: cell_string(row, 3).unwrap_or_default(),
                        definer: None,
                    })
                })
                .collect())
        })
        .await
    }

    async fn list_routines(&self, schema: &str) -> AppResult<Vec<RoutineInfo>> {
        let schema = schema.trim().to_string();
        validate_ident(&schema)?;
        self.with_conn(move |conn| async move {
            let result = conn
                .query(
                    "SELECT object_name, object_type
                     FROM all_objects
                     WHERE owner = :1 AND object_type IN ('FUNCTION', 'PROCEDURE', 'PACKAGE')
                     ORDER BY object_type, object_name",
                    &[Value::String(schema)],
                )
                .await?;
            Ok(result
                .rows
                .iter()
                .filter_map(|row| {
                    Some(RoutineInfo {
                        name: cell_string(row, 0)?,
                        routine_type: cell_string(row, 1).unwrap_or_else(|| "FUNCTION".to_string()),
                        returns: None,
                        deterministic: false,
                        data_access: None,
                        security_type: None,
                        definer: None,
                        created_at: None,
                    })
                })
                .collect())
        })
        .await
    }

    async fn get_columns(&self, schema: &str, table: &str) -> AppResult<Vec<ColumnInfo>> {
        let schema = schema.trim().to_string();
        let table = table.trim().to_string();
        validate_ident(&schema)?;
        validate_ident(&table)?;
        self.with_conn(move |conn| async move {
            let result = conn
                .query(
                    "SELECT c.column_name, c.data_type, c.data_length, c.data_precision, c.data_scale,
                            c.nullable, c.data_default, NVL(cm.comments, ''), c.column_id,
                            CASE WHEN pk.column_name IS NOT NULL THEN 'PRI' ELSE '' END
                     FROM all_tab_columns c
                     LEFT JOIN all_col_comments cm
                       ON cm.owner = c.owner AND cm.table_name = c.table_name AND cm.column_name = c.column_name
                     LEFT JOIN (
                        SELECT cc.owner, cc.table_name, cc.column_name
                        FROM all_cons_columns cc
                        JOIN all_constraints k
                          ON k.owner = cc.owner AND k.constraint_name = cc.constraint_name
                        WHERE k.constraint_type = 'P'
                     ) pk ON pk.owner = c.owner AND pk.table_name = c.table_name AND pk.column_name = c.column_name
                     WHERE c.owner = :1 AND c.table_name = :2
                     ORDER BY c.column_id",
                    &[Value::String(schema), Value::String(table)],
                )
                .await?;
            let mut columns = Vec::new();
            for row in result.rows {
                let Some(name) = cell_string(&row, 0) else {
                    continue;
                };
                let data_type = cell_string(&row, 1).unwrap_or_default();
                let column_type = format_oracle_type(
                    &data_type,
                    cell_i64(&row, 2),
                    cell_i64(&row, 3),
                    cell_i64(&row, 4),
                );
                let default_value = cell_string(&row, 6);
                let extra = if default_value
                    .as_deref()
                    .is_some_and(|value| value.to_ascii_lowercase().contains(".nextval"))
                {
                    "identity".to_string()
                } else {
                    String::new()
                };
                columns.push(ColumnInfo {
                    name,
                    column_type,
                    data_type,
                    nullable: cell_string(&row, 5).is_some_and(|value| value.eq_ignore_ascii_case("Y")),
                    key: cell_string(&row, 9).unwrap_or_default(),
                    default_value,
                    extra,
                    comment: cell_string(&row, 7).unwrap_or_default(),
                    ordinal: cell_u64(&row, 8).unwrap_or(0) as u32,
                });
            }
            Ok(columns)
        })
        .await
    }

    async fn get_ddl(&self, schema: &str, kind: ObjectKind, name: &str) -> AppResult<String> {
        let schema = schema.trim().to_string();
        let name = name.trim().to_string();
        validate_ident(&schema)?;
        validate_ident(&name)?;
        let object_type = match kind {
            ObjectKind::Table => "TABLE",
            ObjectKind::View => "VIEW",
            ObjectKind::Index => "INDEX",
            ObjectKind::Trigger => "TRIGGER",
            ObjectKind::Function => "FUNCTION",
            ObjectKind::Procedure => "PROCEDURE",
        };
        self.with_conn(move |conn| async move {
            let result = conn
                .query(
                    "SELECT DBMS_METADATA.GET_DDL(:1, :2, :3) FROM dual",
                    &[
                        Value::String(object_type.to_string()),
                        Value::String(name.clone()),
                        Value::String(schema.clone()),
                    ],
                )
                .await;
            if let Ok(result) = result {
                if let Some(row) = result.rows.first() {
                    if let Some(value) = row.get(0) {
                        if let Some(ddl) = lob_or_string(&conn, value).await? {
                            let ddl = ddl.trim().to_string();
                            if !ddl.is_empty() {
                                return Ok(ddl);
                            }
                        }
                    }
                }
            }
            if kind == ObjectKind::Table {
                return synthesized_table_ddl(&conn, &schema, &name).await;
            }
            Err(AppError::msg(format!(
                "definition not found for {object_type} {schema}.{name}"
            )))
        })
        .await
    }

    async fn preview_table(
        &self,
        schema: &str,
        table: &str,
        limit: u32,
        offset: u64,
    ) -> AppResult<QueryResult> {
        validate_ident(schema)?;
        validate_ident(table)?;
        let limit = limit.clamp(1, 1_000);
        let sql = format!(
            "SELECT * FROM {} OFFSET {offset} ROWS FETCH NEXT {limit} ROWS ONLY",
            qualify_pg(schema, table)
        );
        self.execute_sql(Some(schema), &sql).await
    }

    async fn table_row_count(&self, schema: &str, table: &str) -> AppResult<u64> {
        let schema = schema.trim().to_string();
        let table = table.trim().to_string();
        validate_ident(&schema)?;
        validate_ident(&table)?;
        let sql = format!("SELECT COUNT(*) FROM {}", qualify_pg(&schema, &table));
        self.with_conn(move |conn| async move {
            let result = conn.query(&sql, &[]).await?;
            Ok(result
                .rows
                .first()
                .and_then(|row| cell_u64(row, 0))
                .unwrap_or(0))
        })
        .await
    }

    async fn execute_sql(&self, schema: Option<&str>, sql: &str) -> AppResult<QueryResult> {
        let sql = sql.trim();
        if sql.is_empty() {
            return Err(AppError::msg("SQL is empty"));
        }
        let limited = apply_default_query_limit(sql, SqlDialect::Oracle);
        let schema_sql = schema
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(oracle_set_schema_sql)
            .transpose()?;
        let statements = split_sql_batch(&limited.sql);
        self.with_conn(move |conn| async move {
            let mut messages = Vec::new();
            if let Some(schema_sql) = &schema_sql {
                run_statement(&conn, schema_sql).await?;
                messages.push(QueryLogEntry::info(schema_sql.clone()));
            }
            if limited.applied {
                messages.push(QueryLogEntry::info(format!(
                    "No row limit specified; applying OFFSET/FETCH NEXT {DEFAULT_QUERY_ROW_LIMIT}"
                )));
            }
            if statements.is_empty() {
                return Err(AppError::msg("SQL is empty"));
            }
            let started = Instant::now();
            let mut affected_rows = 0_u64;
            let mut mutated = false;
            let mut last = empty_result(statement_kind(&limited.sql));
            for statement in &statements {
                messages.push(QueryLogEntry::info(format!(
                    "Executing {}…",
                    statement_kind(statement)
                )));
                if is_explain_plan(statement) {
                    run_statement(&conn, statement).await?;
                    mutated = true;
                    last = read_query(
                        &conn,
                        "SELECT PLAN_TABLE_OUTPUT FROM TABLE(DBMS_XPLAN.DISPLAY)",
                        "query",
                    )
                    .await?;
                    continue;
                }
                if is_row_query(statement) {
                    last = read_query(&conn, statement, statement_kind(statement)).await?;
                    affected_rows += last.affected_rows;
                } else {
                    let count = run_statement(&conn, statement).await?;
                    affected_rows += count;
                    mutated = true;
                    last = empty_result(statement_kind(statement));
                    last.affected_rows = count;
                }
            }
            if mutated {
                conn.commit().await?;
            }
            last.affected_rows = affected_rows;
            last.duration_ms = started.elapsed().as_millis() as u64;
            if limited.applied && last.rows.len() as u32 >= DEFAULT_QUERY_ROW_LIMIT {
                last.truncated = true;
            }
            messages.push(QueryLogEntry::success(format!(
                "Finished in {} ms",
                last.duration_ms
            )));
            last.messages = messages;
            Ok(last)
        })
        .await
    }

    async fn close(self) -> AppResult<()> {
        if let Some(connection) = self.conn.lock().await.take() {
            connection.close().await?;
        }
        Ok(())
    }
}

fn build_config(
    host: &str,
    port: u16,
    username: &str,
    password: Option<&str>,
    database: Option<&str>,
    ssl_ca: Option<&str>,
    verify_cert: bool,
    oracle_version: Option<&str>,
    oracle_connect: Option<&str>,
) -> AppResult<Config> {
    let host = host.trim();
    if host.is_empty() {
        return Err(AppError::msg("host is required"));
    }
    let username = username.trim();
    if username.is_empty() {
        return Err(AppError::msg("username is required"));
    }
    let service = database.map(str::trim).unwrap_or("");
    let (use_sid, name) = oracle_target(service, oracle_connect)?;
    let port = if port == 0 { 1521 } else { port };
    let password = password.unwrap_or("");
    let config = if use_sid {
        Config::with_sid(host, port, name, username, password)
    } else {
        Config::new(host, port, name, username, password)
    };
    Ok(apply_oracle_version(
        apply_oracle_tls(config, verify_cert, ssl_ca)?,
        oracle_version,
    ))
}

#[derive(Debug, PartialEq, Eq)]
enum OracleTls {
    /// Ordinary TCP. No server certificate is checked.
    Plain,
    /// TCPS, validated against the Mozilla trust anchors bundled with the driver.
    VerifySystem,
    /// TCPS, validated against a PEM CA file.
    VerifyCa(String),
}

/// Certificate checks apply only when the user asks to verify.
/// The default leaves verification off, matching SQL Server.
fn oracle_tls(verify_cert: bool, ca: Option<&str>) -> OracleTls {
    if !verify_cert {
        return OracleTls::Plain;
    }
    match ca.map(str::trim).filter(|value| !value.is_empty()) {
        Some(path) => OracleTls::VerifyCa(path.to_string()),
        None => OracleTls::VerifySystem,
    }
}

fn apply_oracle_tls(config: Config, verify_cert: bool, ssl_ca: Option<&str>) -> AppResult<Config> {
    match oracle_tls(verify_cert, ssl_ca) {
        OracleTls::Plain => Ok(config),
        OracleTls::VerifySystem => config
            .with_tls()
            .map_err(|error| AppError::msg(format!("failed to enable Oracle TLS: {error}"))),
        OracleTls::VerifyCa(path) => {
            if !std::path::Path::new(&path).is_file() {
                return Err(AppError::msg(format!("CA certificate not found: {path}")));
            }
            let tls = oracle_rs::TlsConfig::new().with_ca_cert(&path);
            tls.build_client_config().map_err(|error| {
                AppError::msg(format!("failed to load Oracle CA certificate: {error}"))
            })?;
            Ok(config.tls_config(tls))
        }
    }
}

/// Highest TNS version to offer. `None` keeps the driver default (319) and still
/// accepts servers from 10g R1 (311) through 23ai (320).
fn oracle_protocol(version: Option<&str>) -> Option<u16> {
    let value = version?.trim();
    if value.is_empty() || value.eq_ignore_ascii_case("auto") {
        return None;
    }
    let protocol = match value.to_ascii_lowercase().as_str() {
        "23" | "23ai" | "23c" => 320,
        "21" | "21c" => 319,
        "19" | "19c" => 318,
        "18" | "18c" => 317,
        "12.2" | "12c2" | "12cr2" | "122" => 316,
        "12.1" | "12c" | "12c1" | "12cr1" | "121" => 315,
        "11.2" | "11g" | "11gr2" | "112" => 314,
        "11.1" | "11gr1" | "111" => 313,
        "10.2" | "10g" | "10gr2" | "102" => 312,
        "10.1" | "10gr1" | "101" => 311,
        other => other.parse().unwrap_or(0),
    };
    if (311..=320).contains(&protocol) {
        Some(protocol)
    } else {
        None
    }
}

fn apply_oracle_version(config: Config, version: Option<&str>) -> Config {
    match oracle_protocol(version) {
        Some(protocol) => config.protocol_version(protocol),
        None => config,
    }
}

/// Open a connection with the same semantics as JDBC thin:
/// `jdbc:oracle:thin:@host:port:SID` → `(SID=…)` and
/// `jdbc:oracle:thin:@//host:port/service` → `(SERVICE_NAME=…)`.
///
/// 11.2 listeners often close a 12c CONNECT packet without a TNS error.
/// Automatic protocol mode retries once with the 11g packet.
/// When the listener rejects SERVICE_NAME with ORA-12514 (or SID with
/// ORA-12505), retry once with the other identification method — DBeaver
/// SID URLs commonly land in the service-name field by default.
async fn open_connection(config: Config) -> AppResult<Connection> {
    match connect_with_protocol_retry(config.clone()).await {
        Ok(connection) => Ok(connection),
        Err(error) => {
            let Some(flipped) = flip_connect_method(&config, &error) else {
                return Err(error);
            };
            connect_with_protocol_retry(flipped).await
        }
    }
}

async fn connect_with_protocol_retry(config: Config) -> AppResult<Connection> {
    match Connection::connect_with_config(config.clone()).await {
        Ok(connection) => Ok(connection),
        Err(error) if config.protocol_desired == 0 && listener_closed_without_packet(&error) => {
            Connection::connect_with_config(config.protocol_version(314))
                .await
                .map_err(AppError::from)
        }
        Err(error) => Err(AppError::from(error)),
    }
}

fn listener_closed_without_packet(error: &oracle_rs::Error) -> bool {
    match error {
        oracle_rs::Error::ConnectionClosedByServer(message) => message.contains("early eof"),
        oracle_rs::Error::Io(io) => matches!(
            io.kind(),
            std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::UnexpectedEof
                | std::io::ErrorKind::BrokenPipe
        ),
        _ => false,
    }
}

fn flip_connect_method(config: &Config, error: &AppError) -> Option<Config> {
    let AppError::Oracle(oracle_error) = error else {
        return None;
    };
    let mut flipped = config.clone();
    match oracle_error {
        oracle_rs::Error::InvalidServiceName { .. } => {
            let name = config.service.service_name()?.to_string();
            flipped.service = oracle_rs::ServiceMethod::Sid(name);
            Some(flipped)
        }
        oracle_rs::Error::InvalidSid { .. } => {
            let name = config.service.sid()?.to_string();
            flipped.service = oracle_rs::ServiceMethod::ServiceName(name);
            Some(flipped)
        }
        _ => None,
    }
}

/// `sid:` prefix or connect mode `sid` selects the instance SID.
/// Anything else is a service name.
fn oracle_target<'a>(database: &'a str, connect_as: Option<&str>) -> AppResult<(bool, &'a str)> {
    if database.starts_with('(') {
        return Err(AppError::msg(
            "Oracle connections use a service name or SID, not a full connect descriptor",
        ));
    }
    let (forced_sid, name) = match database.strip_prefix("sid:") {
        Some(sid) => (true, sid.trim()),
        None => (false, database),
    };
    let use_sid = forced_sid
        || connect_as
            .map(str::trim)
            .is_some_and(|value| value.eq_ignore_ascii_case("sid"));
    if name.is_empty() {
        return Err(AppError::msg(if use_sid {
            "Oracle SID is required"
        } else {
            "Oracle service name is required"
        }));
    }
    Ok((use_sid, name))
}

fn is_oracle_system(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    SYSTEM_USERS.iter().any(|item| *item == upper)
        || upper.starts_with("APEX_")
        || upper.starts_with("FLOWS_")
        || upper.starts_with("ORDS_")
}

fn format_oracle_type(
    data_type: &str,
    length: Option<i64>,
    precision: Option<i64>,
    scale: Option<i64>,
) -> String {
    match data_type {
        "VARCHAR2" | "NVARCHAR2" | "CHAR" | "NCHAR" | "RAW" => match length {
            Some(length) if length > 0 => format!("{data_type}({length})"),
            _ => data_type.to_string(),
        },
        "NUMBER" => match (precision, scale) {
            (Some(precision), Some(scale)) if scale != 0 => format!("NUMBER({precision},{scale})"),
            (Some(precision), _) => format!("NUMBER({precision})"),
            _ => "NUMBER".to_string(),
        },
        "FLOAT" => match precision {
            Some(precision) => format!("FLOAT({precision})"),
            None => "FLOAT".to_string(),
        },
        _ => data_type.to_string(),
    }
}

fn cell_string(row: &Row, index: usize) -> Option<String> {
    row.get(index).and_then(value_to_string)
}

fn cell_i64(row: &Row, index: usize) -> Option<i64> {
    row.get(index).and_then(Value::as_i64)
}

fn cell_u64(row: &Row, index: usize) -> Option<u64> {
    cell_i64(row, index).map(|value| value.max(0) as u64)
}

fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(value) => Some(value.clone()),
        Value::Integer(value) => Some(value.to_string()),
        Value::Float(value) => Some(value.to_string()),
        Value::Number(value) => Some(value.as_str().to_string()),
        Value::Boolean(value) => Some(if *value { "1" } else { "0" }.to_string()),
        Value::Date(value) => Some(format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            value.year, value.month, value.day, value.hour, value.minute, value.second
        )),
        Value::Timestamp(value) => Some(format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:06}",
            value.year,
            value.month,
            value.day,
            value.hour,
            value.minute,
            value.second,
            value.microsecond
        )),
        Value::Bytes(value) => Some(to_hex(value)),
        Value::Json(value) => Some(value.to_string()),
        Value::RowId(value) => value.to_string(),
        Value::Lob(LobValue::Inline(bytes)) => Some(String::from_utf8_lossy(bytes).into_owned()),
        Value::Lob(LobValue::Empty | LobValue::Null) => None,
        Value::Lob(LobValue::Locator(_)) => Some("[LOB]".to_string()),
        other => Some(format!("{other:?}")),
    }
}

fn to_hex(bytes: &[u8]) -> String {
    const MAX: usize = 64;
    let mut out = String::from("0x");
    for byte in bytes.iter().take(MAX) {
        out.push_str(&format!("{byte:02X}"));
    }
    if bytes.len() > MAX {
        out.push_str("…");
    }
    out
}

async fn lob_or_string(conn: &Connection, value: &Value) -> AppResult<Option<String>> {
    match value {
        Value::Lob(LobValue::Locator(locator)) => Ok(Some(conn.read_clob(locator).await?)),
        other => Ok(value_to_string(other)),
    }
}

fn is_row_query(sql: &str) -> bool {
    matches!(statement_kind(sql), "query")
        && !sql.trim_start().to_ascii_uppercase().starts_with("EXPLAIN")
}

fn is_explain_plan(sql: &str) -> bool {
    sql.trim_start()
        .to_ascii_uppercase()
        .starts_with("EXPLAIN PLAN")
}

fn is_plsql(sql: &str) -> bool {
    let upper = sql.trim_start().to_ascii_uppercase();
    upper.starts_with("BEGIN") || upper.starts_with("DECLARE") || upper.starts_with("CALL")
}

async fn run_statement(conn: &Connection, sql: &str) -> AppResult<u64> {
    if is_plsql(sql) {
        let params: &[BindParam] = &[];
        let result = conn.execute_plsql(sql, params).await?;
        return Ok(result.rows_affected);
    }
    let result = conn.execute(sql, &[]).await?;
    Ok(result.rows_affected)
}

async fn read_query(conn: &Connection, sql: &str, kind: &str) -> AppResult<QueryResult> {
    let mut result = conn.query(sql, &[]).await?;
    let mut truncated = false;
    while result.has_more_rows && result.rows.len() < MAX_RESULT_ROWS {
        let more = conn
            .fetch_more(result.cursor_id, &result.columns, 200)
            .await?;
        let has_more = more.has_more_rows;
        result.rows.extend(more.rows);
        result.has_more_rows = has_more;
        if result.rows.len() >= MAX_RESULT_ROWS {
            truncated = true;
            result.rows.truncate(MAX_RESULT_ROWS);
            break;
        }
    }
    let columns = result.columns.iter().map(column_meta).collect::<Vec<_>>();
    let rows = result
        .rows
        .iter()
        .map(|row| {
            (0..columns.len())
                .map(|index| cell_string(row, index))
                .collect()
        })
        .collect();
    Ok(QueryResult {
        columns,
        rows,
        affected_rows: result.rows_affected,
        last_insert_id: None,
        duration_ms: 0,
        truncated,
        statement_kind: kind.to_string(),
        messages: Vec::new(),
    })
}

fn column_meta(column: &OracleColumn) -> ColumnMeta {
    ColumnMeta {
        name: column.name.clone(),
        type_name: format!("{:?}", column.oracle_type),
    }
}

fn empty_result(kind: &str) -> QueryResult {
    QueryResult {
        columns: Vec::new(),
        rows: Vec::new(),
        affected_rows: 0,
        last_insert_id: None,
        duration_ms: 0,
        truncated: false,
        statement_kind: kind.to_string(),
        messages: Vec::new(),
    }
}

async fn synthesized_table_ddl(conn: &Connection, schema: &str, table: &str) -> AppResult<String> {
    let result = conn
        .query(
            "SELECT column_name, data_type, data_length, data_precision, data_scale, nullable
             FROM all_tab_columns
             WHERE owner = :1 AND table_name = :2
             ORDER BY column_id",
            &[
                Value::String(schema.to_string()),
                Value::String(table.to_string()),
            ],
        )
        .await?;
    let mut lines = Vec::new();
    for row in &result.rows {
        let Some(name) = cell_string(row, 0) else {
            continue;
        };
        let data_type = cell_string(row, 1).unwrap_or_else(|| "VARCHAR2".to_string());
        let column_type = format_oracle_type(
            &data_type,
            cell_i64(row, 2),
            cell_i64(row, 3),
            cell_i64(row, 4),
        );
        let null_sql = if cell_string(row, 5).is_some_and(|value| value.eq_ignore_ascii_case("Y")) {
            ""
        } else {
            " NOT NULL"
        };
        lines.push(format!(
            "  {} {column_type}{null_sql}",
            quote_ident_pg(&name)
        ));
    }
    if lines.is_empty() {
        return Err(AppError::msg(format!("table not found: {schema}.{table}")));
    }
    Ok(format!(
        "CREATE TABLE {} (\n{}\n)",
        qualify_pg(schema, table),
        lines.join(",\n")
    ))
}

fn split_sql_batch(sql: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut depth = 0_i32;
    let mut in_declare = false;
    let mut quote: Option<char> = None;
    let chars: Vec<(usize, char)> = sql.char_indices().collect();
    let mut cursor = 0;
    while cursor < chars.len() {
        let (byte, ch) = chars[cursor];
        if let Some(active) = quote {
            if ch == active {
                if chars
                    .get(cursor + 1)
                    .is_some_and(|(_, next)| *next == active)
                {
                    cursor += 2;
                    continue;
                }
                quote = None;
            }
            cursor += 1;
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            ';' if depth == 0 && !in_declare => {
                push_statement(&mut out, &sql[start..byte]);
                start = chars
                    .get(cursor + 1)
                    .map(|(next, _)| *next)
                    .unwrap_or(sql.len());
            }
            _ => {
                if keyword_at(sql, byte, "DECLARE") && depth == 0 {
                    in_declare = true;
                } else if keyword_at(sql, byte, "BEGIN") {
                    in_declare = false;
                    depth += 1;
                } else if depth > 0 && keyword_at(sql, byte, "END") {
                    depth -= 1;
                }
            }
        }
        cursor += 1;
    }
    push_statement(&mut out, &sql[start..]);
    out
}

fn push_statement(out: &mut Vec<String>, raw: &str) {
    let mut statement = raw.trim().trim_end_matches(';').trim().to_string();
    if statement.is_empty() {
        return;
    }
    let upper = statement.to_ascii_uppercase();
    if upper.starts_with("BEGIN") || upper.starts_with("DECLARE") {
        statement.push(';');
    }
    out.push(statement);
}

fn keyword_at(sql: &str, byte: usize, word: &str) -> bool {
    let rest = &sql[byte..];
    if rest.len() < word.len() || !rest[..word.len()].eq_ignore_ascii_case(word) {
        return false;
    }
    let before_ok = byte == 0
        || sql[..byte]
            .chars()
            .next_back()
            .is_none_or(|ch| !ch.is_ascii_alphanumeric() && ch != '_');
    let after = rest[word.len()..].chars().next();
    let after_ok = after.is_none_or(|ch| !ch.is_ascii_alphanumeric() && ch != '_');
    before_ok && after_ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn certificate_verification_is_optional() {
        assert_eq!(oracle_tls(false, None), OracleTls::Plain);
        assert_eq!(oracle_tls(false, Some("ca.pem")), OracleTls::Plain);
        assert_eq!(oracle_tls(false, Some("  ")), OracleTls::Plain);
        assert_eq!(oracle_tls(true, None), OracleTls::VerifySystem);
        assert_eq!(oracle_tls(true, Some("  ")), OracleTls::VerifySystem);
        assert_eq!(
            oracle_tls(true, Some("ca.pem")),
            OracleTls::VerifyCa("ca.pem".to_string())
        );
    }

    #[test]
    fn default_connection_does_not_require_a_trusted_certificate() {
        let config = apply_oracle_tls(
            Config::new("localhost", 1521, "FREEPDB1", "system", "secret"),
            false,
            Some("ca.pem"),
        )
        .unwrap();
        assert!(!config.is_tls_enabled());
    }

    #[test]
    fn verification_enables_tls_against_system_roots() {
        let config = apply_oracle_tls(
            Config::new("localhost", 1521, "FREEPDB1", "system", "secret"),
            true,
            None,
        )
        .unwrap();
        assert!(config.is_tls_enabled());
        assert!(config.tls_config.unwrap().verify_server);
    }

    #[test]
    fn rejects_missing_ca_when_verification_is_on() {
        let error = apply_oracle_tls(
            Config::new("localhost", 1521, "FREEPDB1", "system", "secret"),
            true,
            Some("/definitely/missing/ca.pem"),
        )
        .unwrap_err();
        assert!(error.to_string().contains("CA certificate not found"));
    }

    #[test]
    fn selects_a_protocol_for_each_oracle_release() {
        assert_eq!(oracle_protocol(None), None);
        assert_eq!(oracle_protocol(Some("auto")), None);
        assert_eq!(oracle_protocol(Some("11.2")), Some(314));
        assert_eq!(oracle_protocol(Some("11g")), Some(314));
        assert_eq!(oracle_protocol(Some("10.1")), Some(311));
        assert_eq!(oracle_protocol(Some("23ai")), Some(320));
        assert_eq!(oracle_protocol(Some("19c")), Some(318));
        assert_eq!(oracle_protocol(Some("12.2")), Some(316));
        assert_eq!(oracle_protocol(Some("nope")), None);
    }

    #[test]
    fn retries_only_the_automatic_protocol_after_a_silent_listener_close() {
        let silent = oracle_rs::Error::ConnectionClosedByServer(
            "I/O error: early eof. The listener closed the connection without sending an error packet.".into(),
        );
        let refused = oracle_rs::Error::InvalidServiceName {
            service_name: Some("DB11G".into()),
            message: None,
        };
        assert!(listener_closed_without_packet(&silent));
        assert!(!listener_closed_without_packet(&refused));
    }

    #[test]
    fn eleven_g_offers_protocol_314() {
        let config = apply_oracle_version(
            Config::new("localhost", 1521, "FREEPDB1", "system", "secret"),
            Some("11.2"),
        );
        assert_eq!(config.protocol_desired, 314);
    }

    #[test]
    fn connects_by_service_name_or_sid() {
        let service = build_config(
            "localhost",
            1521,
            "system",
            Some("secret"),
            Some("FREEPDB1"),
            None,
            false,
            None,
            Some("service"),
        )
        .unwrap();
        assert_eq!(service.service.service_name(), Some("FREEPDB1"));
        assert!(service
            .build_connect_string()
            .contains("(SERVICE_NAME=FREEPDB1)"));

        let sid = build_config(
            "localhost",
            1521,
            "system",
            Some("secret"),
            Some("ORCL"),
            None,
            false,
            None,
            Some("sid"),
        )
        .unwrap();
        assert_eq!(sid.service.sid(), Some("ORCL"));
        let sid_descriptor = sid.build_connect_string();
        assert!(sid_descriptor.contains("(SID=ORCL)"));
        assert!(!sid_descriptor.contains("SERVICE_NAME"));
        // Match JDBC thin `@host:port:SID` — no forced SERVER=DEDICATED.
        assert!(!sid_descriptor.contains("SERVER="));

        let jdbc_style = build_config(
            "172.19.3.11",
            7026,
            "system",
            Some("secret"),
            Some("DB11G"),
            None,
            false,
            Some("11.2"),
            Some("sid"),
        )
        .unwrap();
        assert_eq!(jdbc_style.service.sid(), Some("DB11G"));
        assert_eq!(jdbc_style.protocol_desired, 314);
        assert_eq!(
            jdbc_style.build_connect_string(),
            "(DESCRIPTION=(ADDRESS=(PROTOCOL=TCP)(HOST=172.19.3.11)(PORT=7026))(CONNECT_DATA=(SID=DB11G)))"
        );

        let prefixed = build_config(
            "localhost",
            1521,
            "system",
            None,
            Some("sid:ORCL"),
            None,
            false,
            None,
            None,
        )
        .unwrap();
        assert_eq!(prefixed.service.sid(), Some("ORCL"));
    }

    #[test]
    fn flips_service_name_to_sid_after_ora_12514() {
        let config = Config::new("172.19.3.11", 7026, "DB11G", "system", "secret");
        let error = AppError::from(oracle_rs::Error::InvalidServiceName {
            service_name: Some("DB11G".into()),
            message: None,
        });
        let flipped = flip_connect_method(&config, &error).expect("flip to SID");
        assert_eq!(flipped.service.sid(), Some("DB11G"));
        assert!(flipped.build_connect_string().contains("(SID=DB11G)"));
    }

    #[test]
    fn flips_sid_to_service_name_after_ora_12505() {
        let config = Config::with_sid("172.19.3.11", 7026, "DB11G", "system", "secret");
        let error = AppError::from(oracle_rs::Error::InvalidSid {
            sid: Some("DB11G".into()),
            message: None,
        });
        let flipped = flip_connect_method(&config, &error).expect("flip to service");
        assert_eq!(flipped.service.service_name(), Some("DB11G"));
    }
}
