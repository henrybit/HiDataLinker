mod values;

use super::dump::{build_dump, build_table_data_dump, DumpDialect};
use super::engine::DatabaseEngine;
use super::ident::{
    create_mssql_database_sql, drop_mssql_database_sql, mssql_schema_object, qualify_mssql,
    quote_ident_mssql, validate_ident,
};
use super::sql::{apply_default_query_limit, statement_kind, SqlDialect, DEFAULT_QUERY_ROW_LIMIT};
use crate::error::{AppError, AppResult};
use crate::models::{
    CharsetCatalog, CharsetInfo, CollationInfo, ColumnInfo, ColumnMeta, ConnectionProfile,
    DatabaseInfo, IndexInfo, ObjectKind, QueryLogEntry, QueryResult, RoutineInfo, TableInfo,
    TestConnectionRequest, TriggerInfo, ViewInfo,
};
use async_trait::async_trait;
use deadpool::managed::{Manager, Metrics, Pool, RecycleError, RecycleResult};
use futures_util::TryStreamExt;
use std::time::Instant;
use tiberius::{AuthMethod, Client, Config, EncryptionLevel, QueryItem};
use tokio::net::TcpStream;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};
use values::{cell_to_string, parse_bool, parse_u64};

const SYSTEM_DATABASES: [&str; 4] = ["master", "model", "msdb", "tempdb"];
const MAX_RESULT_ROWS: usize = 5_000;

type MsClient = Client<Compat<TcpStream>>;

#[derive(Clone)]
struct MsParams {
    host: String,
    port: u16,
    username: String,
    password: String,
    database: String,
    verify_cert: bool,
    ca: Option<String>,
}

#[derive(Clone)]
struct MssqlManager {
    params: MsParams,
}

impl Manager for MssqlManager {
    type Type = MsClient;
    type Error = AppError;

    fn create(&self) -> impl std::future::Future<Output = Result<Self::Type, Self::Error>> + Send {
        let params = self.params.clone();
        async move { connect_client(&params).await }
    }

    fn recycle(
        &self,
        client: &mut Self::Type,
        _: &Metrics,
    ) -> impl std::future::Future<Output = RecycleResult<Self::Error>> + Send {
        let recycle = async move {
            let stream = client
                .simple_query("SELECT 1")
                .await
                .map_err(|error| RecycleError::message(error.to_string()))?;
            stream
                .into_results()
                .await
                .map_err(|error| RecycleError::message(error.to_string()))?;
            Ok(())
        };
        recycle
    }
}

#[derive(Clone)]
pub struct SqlServerEngine {
    pool: Pool<MssqlManager>,
    database: String,
}

impl SqlServerEngine {
    pub fn from_profile(profile: &ConnectionProfile) -> AppResult<Self> {
        let params = params_from(
            &profile.host,
            profile.port,
            &profile.username,
            profile.password.as_deref(),
            profile.database.as_deref(),
            profile.ssl_ca.as_deref(),
            profile.ssl_verify,
        )?;
        Self::new(params)
    }

    pub async fn test(request: &TestConnectionRequest) -> AppResult<()> {
        let params = params_from(
            &request.host,
            request.port,
            &request.username,
            request.password.as_deref(),
            request.database.as_deref(),
            request.ssl_ca.as_deref(),
            request.ssl_verify,
        )?;
        let mut client = connect_client(&params).await?;
        let stream = client.simple_query("SELECT 1").await?;
        stream.into_results().await?;
        Ok(())
    }

    fn new(params: MsParams) -> AppResult<Self> {
        let database = params.database.clone();
        let pool = Pool::builder(MssqlManager { params })
            .max_size(8)
            .build()
            .map_err(|error| AppError::msg(format!("failed to create SQL Server pool: {error}")))?;
        Ok(Self { pool, database })
    }

    async fn client(&self) -> AppResult<deadpool::managed::Object<MssqlManager>> {
        self.pool
            .get()
            .await
            .map_err(|error| AppError::msg(error.to_string()))
    }

    async fn use_database(client: &mut MsClient, database: &str) -> AppResult<()> {
        validate_ident(database)?;
        let sql = format!("USE {}", quote_ident_mssql(database));
        let stream = client.simple_query(sql).await?;
        stream.into_results().await?;
        Ok(())
    }
}

#[async_trait]
impl DatabaseEngine for SqlServerEngine {
    async fn ping(&self) -> AppResult<()> {
        let mut client = self.client().await?;
        let stream = client.simple_query("SELECT 1").await?;
        stream.into_results().await?;
        Ok(())
    }

    async fn list_databases(&self) -> AppResult<Vec<DatabaseInfo>> {
        let mut client = self.client().await?;
        let rows = query_rows(
            &mut client,
            "SELECT name, collation_name FROM sys.databases ORDER BY name",
        )
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                let name = cell_to_string(&row, 0)?;
                let collation = cell_to_string(&row, 1);
                Some(DatabaseInfo {
                    is_system: SYSTEM_DATABASES
                        .iter()
                        .any(|item| item.eq_ignore_ascii_case(&name)),
                    name,
                    charset: None,
                    collation,
                })
            })
            .collect())
    }

    async fn create_database(
        &self,
        name: &str,
        _charset: Option<&str>,
        collation: Option<&str>,
    ) -> AppResult<()> {
        let sql = create_mssql_database_sql(name, collation)?;
        let mut client = self.client().await?;
        let stream = client.simple_query(sql).await?;
        stream.into_results().await?;
        Ok(())
    }

    async fn drop_database(&self, name: &str) -> AppResult<()> {
        let name = name.trim();
        validate_ident(name)?;
        if SYSTEM_DATABASES
            .iter()
            .any(|item| item.eq_ignore_ascii_case(name))
        {
            return Err(AppError::msg(format!(
                "cannot drop system database: {name}"
            )));
        }
        let sql = drop_mssql_database_sql(name)?;
        let mut client = self.client().await?;
        SqlServerEngine::use_database(&mut client, "master").await?;
        let stream = client.simple_query(sql).await?;
        stream.into_results().await?;
        Ok(())
    }

    async fn dump_database(
        &self,
        name: &str,
        include_schema: bool,
        include_data: bool,
    ) -> AppResult<String> {
        let name = name.trim();
        validate_ident(name)?;
        build_dump(self, name, include_schema, include_data, DumpDialect::Mssql).await
    }

    async fn dump_table(&self, schema: &str, table: &str) -> AppResult<String> {
        validate_ident(schema)?;
        validate_ident(table)?;
        build_table_data_dump(self, schema, table, DumpDialect::Mssql).await
    }

    async fn list_charset_catalog(&self) -> AppResult<CharsetCatalog> {
        let mut client = self.client().await?;
        let rows = query_rows(
            &mut client,
            "SELECT name FROM sys.fn_helpcollations() ORDER BY name",
        )
        .await?;
        let collations = rows
            .into_iter()
            .filter_map(|row| {
                let name = cell_to_string(&row, 0)?;
                Some(CollationInfo {
                    is_default: name.eq_ignore_ascii_case("SQL_Latin1_General_CP1_CI_AS"),
                    name,
                    charset: "collation".to_string(),
                })
            })
            .collect();
        Ok(CharsetCatalog {
            charsets: vec![CharsetInfo {
                name: "collation".to_string(),
                default_collation: Some("SQL_Latin1_General_CP1_CI_AS".to_string()),
                description: Some("SQL Server collation".to_string()),
            }],
            collations,
        })
    }

    async fn list_tables(&self, schema: &str) -> AppResult<Vec<TableInfo>> {
        let database = checked_database(schema)?;
        let sql = format!(
            "SELECT s.name, t.name,
                    CONVERT(varchar(19), t.create_date, 120),
                    CONVERT(varchar(19), t.modify_date, 120),
                    SUM(CASE WHEN p.index_id IN (0, 1) THEN p.rows ELSE 0 END),
                    SUM(a.used_pages) * 8192,
                    CONVERT(nvarchar(4000), ep.value)
             FROM {database}.sys.tables t
             JOIN {database}.sys.schemas s ON s.schema_id = t.schema_id
             LEFT JOIN {database}.sys.partitions p ON p.object_id = t.object_id
             LEFT JOIN {database}.sys.allocation_units a ON a.container_id = p.partition_id
             LEFT JOIN {database}.sys.extended_properties ep
               ON ep.major_id = t.object_id AND ep.minor_id = 0 AND ep.name = N'MS_Description'
             WHERE t.is_ms_shipped = 0
             GROUP BY s.name, t.name, t.create_date, t.modify_date, ep.value
             ORDER BY s.name, t.name"
        );
        let mut client = self.client().await?;
        let rows = query_rows(&mut client, &sql).await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                let schema_name = cell_to_string(&row, 0)?;
                let table_name = cell_to_string(&row, 1)?;
                Some(TableInfo {
                    name: format!("{schema_name}.{table_name}"),
                    engine: Some("SQL Server".to_string()),
                    created_at: cell_to_string(&row, 2),
                    updated_at: cell_to_string(&row, 3),
                    table_rows: parse_u64(cell_to_string(&row, 4)),
                    data_length: parse_u64(cell_to_string(&row, 5)),
                    comment: cell_to_string(&row, 6).unwrap_or_default(),
                })
            })
            .collect())
    }

    async fn list_views(&self, schema: &str) -> AppResult<Vec<ViewInfo>> {
        let database = checked_database(schema)?;
        let sql = format!(
            "SELECT s.name, v.name
             FROM {database}.sys.views v
             JOIN {database}.sys.schemas s ON s.schema_id = v.schema_id
             WHERE v.is_ms_shipped = 0
             ORDER BY s.name, v.name"
        );
        let mut client = self.client().await?;
        let rows = query_rows(&mut client, &sql).await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                let schema_name = cell_to_string(&row, 0)?;
                let name = cell_to_string(&row, 1)?;
                Some(ViewInfo {
                    name: format!("{schema_name}.{name}"),
                    updatable: false,
                    check_option: None,
                    security_type: None,
                    definer: None,
                })
            })
            .collect())
    }

    async fn list_indexes(&self, schema: &str) -> AppResult<Vec<IndexInfo>> {
        let database = checked_database(schema)?;
        let sql = format!(
            "SELECT i.name, s.name, t.name, i.is_unique, i.is_primary_key, i.type_desc, c.name
             FROM {database}.sys.indexes i
             JOIN {database}.sys.tables t ON t.object_id = i.object_id
             JOIN {database}.sys.schemas s ON s.schema_id = t.schema_id
             JOIN {database}.sys.index_columns ic
               ON ic.object_id = i.object_id AND ic.index_id = i.index_id AND ic.is_included_column = 0
             JOIN {database}.sys.columns c ON c.object_id = ic.object_id AND c.column_id = ic.column_id
             WHERE i.name IS NOT NULL AND t.is_ms_shipped = 0
             ORDER BY s.name, t.name, i.name, ic.key_ordinal"
        );
        let mut client = self.client().await?;
        let rows = query_rows(&mut client, &sql).await?;
        let mut indexes = Vec::<IndexInfo>::new();
        for row in rows {
            let Some(name) = cell_to_string(&row, 0) else {
                continue;
            };
            let Some(schema_name) = cell_to_string(&row, 1) else {
                continue;
            };
            let Some(table_name) = cell_to_string(&row, 2) else {
                continue;
            };
            let qualified = format!("{schema_name}.{table_name}");
            let column = cell_to_string(&row, 6).unwrap_or_default();
            if let Some(last) = indexes.last_mut() {
                if last.name == name && last.table_name == qualified {
                    last.columns.push(column);
                    continue;
                }
            }
            indexes.push(IndexInfo {
                name,
                table_name: qualified,
                unique: parse_bool(cell_to_string(&row, 3)),
                primary: parse_bool(cell_to_string(&row, 4)),
                index_type: cell_to_string(&row, 5).unwrap_or_else(|| "INDEX".to_string()),
                columns: vec![column],
                comment: String::new(),
            });
        }
        Ok(indexes)
    }

    async fn list_triggers(&self, schema: &str) -> AppResult<Vec<TriggerInfo>> {
        let database = checked_database(schema)?;
        let sql = format!(
            "SELECT tr.name, s.name, t.name,
                    CASE WHEN tr.is_instead_of_trigger = 1 THEN 'INSTEAD OF' ELSE 'AFTER' END,
                    te.type_desc
             FROM {database}.sys.triggers tr
             JOIN {database}.sys.tables t ON t.object_id = tr.parent_id
             JOIN {database}.sys.schemas s ON s.schema_id = t.schema_id
             LEFT JOIN {database}.sys.trigger_events te ON te.object_id = tr.object_id
             WHERE tr.is_ms_shipped = 0
             ORDER BY s.name, tr.name, te.type_desc"
        );
        let mut client = self.client().await?;
        let rows = query_rows(&mut client, &sql).await?;
        let mut triggers = Vec::<TriggerInfo>::new();
        for row in rows {
            let Some(name) = cell_to_string(&row, 0) else {
                continue;
            };
            let Some(schema_name) = cell_to_string(&row, 1) else {
                continue;
            };
            let Some(table_name) = cell_to_string(&row, 2) else {
                continue;
            };
            let qualified_name = format!("{schema_name}.{name}");
            let event = cell_to_string(&row, 4).unwrap_or_default();
            if let Some(last) = triggers.last_mut() {
                if last.name == qualified_name {
                    if !event.is_empty() && !last.event.split(',').any(|item| item == event) {
                        if !last.event.is_empty() {
                            last.event.push(',');
                        }
                        last.event.push_str(&event);
                    }
                    continue;
                }
            }
            triggers.push(TriggerInfo {
                name: qualified_name,
                table_name: format!("{schema_name}.{table_name}"),
                timing: cell_to_string(&row, 3).unwrap_or_else(|| "AFTER".to_string()),
                event,
                definer: None,
            });
        }
        Ok(triggers)
    }

    async fn list_routines(&self, schema: &str) -> AppResult<Vec<RoutineInfo>> {
        let database = checked_database(schema)?;
        let sql = format!(
            "SELECT s.name, o.name,
                    CASE WHEN o.type = 'P' THEN 'PROCEDURE' ELSE 'FUNCTION' END
             FROM {database}.sys.objects o
             JOIN {database}.sys.schemas s ON s.schema_id = o.schema_id
             WHERE o.type IN ('P', 'FN', 'TF', 'IF') AND o.is_ms_shipped = 0
             ORDER BY o.type, s.name, o.name"
        );
        let mut client = self.client().await?;
        let rows = query_rows(&mut client, &sql).await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                let schema_name = cell_to_string(&row, 0)?;
                let name = cell_to_string(&row, 1)?;
                Some(RoutineInfo {
                    name: format!("{schema_name}.{name}"),
                    routine_type: cell_to_string(&row, 2).unwrap_or_else(|| "FUNCTION".to_string()),
                    returns: None,
                    deterministic: false,
                    data_access: None,
                    security_type: None,
                    definer: None,
                    created_at: None,
                })
            })
            .collect())
    }

    async fn get_columns(&self, schema: &str, table: &str) -> AppResult<Vec<ColumnInfo>> {
        let database = checked_database(schema)?;
        let (object_schema, object_name) = mssql_schema_object(table);
        validate_ident(object_schema)?;
        validate_ident(object_name)?;
        let sql = format!(
            "SELECT c.name, ty.name, c.max_length, c.precision, c.scale, c.is_nullable, c.is_identity,
                    dc.definition, CONVERT(nvarchar(4000), ep.value), c.column_id,
                    CASE WHEN pk.column_id IS NOT NULL THEN 'PRI'
                         WHEN ix.column_id IS NOT NULL THEN 'MUL' ELSE '' END
             FROM {database}.sys.columns c
             JOIN {database}.sys.tables t ON t.object_id = c.object_id
             JOIN {database}.sys.schemas s ON s.schema_id = t.schema_id
             JOIN {database}.sys.types ty ON ty.user_type_id = c.user_type_id
             LEFT JOIN {database}.sys.default_constraints dc ON dc.object_id = c.default_object_id
             LEFT JOIN {database}.sys.extended_properties ep
               ON ep.major_id = c.object_id AND ep.minor_id = c.column_id AND ep.name = N'MS_Description'
             LEFT JOIN (
                SELECT ic.object_id, ic.column_id
                FROM {database}.sys.indexes i
                JOIN {database}.sys.index_columns ic
                  ON ic.object_id = i.object_id AND ic.index_id = i.index_id
                WHERE i.is_primary_key = 1
             ) pk ON pk.object_id = c.object_id AND pk.column_id = c.column_id
             LEFT JOIN (
                SELECT ic.object_id, ic.column_id
                FROM {database}.sys.indexes i
                JOIN {database}.sys.index_columns ic
                  ON ic.object_id = i.object_id AND ic.index_id = i.index_id
                WHERE i.is_primary_key = 0 AND i.is_unique = 0
             ) ix ON ix.object_id = c.object_id AND ix.column_id = c.column_id
             WHERE s.name = {schema_lit} AND t.name = {table_lit}
             ORDER BY c.column_id",
            schema_lit = nquote(object_schema),
            table_lit = nquote(object_name),
        );
        let mut client = self.client().await?;
        let rows = query_rows(&mut client, &sql).await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                let name = cell_to_string(&row, 0)?;
                let data_type = cell_to_string(&row, 1).unwrap_or_default();
                let max_length = parse_u64(cell_to_string(&row, 2)).unwrap_or(0) as i32;
                let precision = parse_u64(cell_to_string(&row, 3)).unwrap_or(0) as u8;
                let scale = parse_u64(cell_to_string(&row, 4)).unwrap_or(0) as u8;
                let column_type = format_mssql_type(&data_type, max_length, precision, scale);
                let identity = parse_bool(cell_to_string(&row, 6));
                Some(ColumnInfo {
                    name,
                    column_type: column_type.clone(),
                    data_type,
                    nullable: parse_bool(cell_to_string(&row, 5)),
                    key: cell_to_string(&row, 10).unwrap_or_default(),
                    default_value: cell_to_string(&row, 7),
                    extra: if identity {
                        "identity".to_string()
                    } else {
                        String::new()
                    },
                    comment: cell_to_string(&row, 8).unwrap_or_default(),
                    ordinal: parse_u64(cell_to_string(&row, 9)).unwrap_or(0) as u32,
                })
            })
            .collect())
    }

    async fn get_ddl(&self, schema: &str, kind: ObjectKind, name: &str) -> AppResult<String> {
        let database = checked_database(schema)?;
        match kind {
            ObjectKind::Table => table_ddl(self, &database, name).await,
            ObjectKind::Index => index_ddl(self, &database, name).await,
            ObjectKind::View
            | ObjectKind::Trigger
            | ObjectKind::Function
            | ObjectKind::Procedure => module_ddl(self, &database, name).await,
        }
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
            "SELECT * FROM {} ORDER BY (SELECT NULL) OFFSET {offset} ROWS FETCH NEXT {limit} ROWS ONLY",
            qualify_mssql(schema, table)
        );
        self.execute_sql(Some(schema), &sql).await
    }

    async fn table_row_count(&self, schema: &str, table: &str) -> AppResult<u64> {
        validate_ident(schema)?;
        validate_ident(table)?;
        let sql = format!("SELECT COUNT_BIG(*) FROM {}", qualify_mssql(schema, table));
        let mut client = self.client().await?;
        let rows = query_rows(&mut client, &sql).await?;
        let count = rows
            .first()
            .and_then(|row| parse_u64(cell_to_string(row, 0)))
            .unwrap_or(0);
        Ok(count)
    }

    async fn execute_sql(&self, schema: Option<&str>, sql: &str) -> AppResult<QueryResult> {
        let sql = sql.trim();
        if sql.is_empty() {
            return Err(AppError::msg("SQL is empty"));
        }
        let limited = apply_default_query_limit(sql, SqlDialect::Mssql);
        let mut messages = Vec::new();
        let mut client = self.client().await?;
        if let Some(schema) = schema {
            SqlServerEngine::use_database(&mut client, schema).await?;
            messages.push(QueryLogEntry::info(format!("USE {schema}")));
        }
        if limited.applied {
            messages.push(QueryLogEntry::info(format!(
                "No row limit specified; applying OFFSET/FETCH NEXT {DEFAULT_QUERY_ROW_LIMIT}"
            )));
        }
        messages.push(QueryLogEntry::info(format!(
            "Executing {}…",
            statement_kind(&limited.sql)
        )));
        let started = Instant::now();
        let show_rows = statement_kind(&limited.sql) == "query"
            || limited
                .sql
                .to_ascii_uppercase()
                .contains("STATISTICS PROFILE");
        let mut result = if show_rows {
            let stream = client.simple_query(limited.sql.as_str()).await?;
            collect_query(
                stream,
                &limited.sql,
                started.elapsed().as_millis() as u64,
                messages,
            )
            .await?
        } else {
            let executed = client.execute(limited.sql.as_str(), &[]).await?;
            let mut result = QueryResult {
                columns: Vec::new(),
                rows: Vec::new(),
                affected_rows: executed.total(),
                last_insert_id: None,
                duration_ms: started.elapsed().as_millis() as u64,
                truncated: false,
                statement_kind: statement_kind(&limited.sql).to_string(),
                messages,
            };
            result.messages.push(QueryLogEntry::success(format!(
                "{} finished in {} ms",
                result.statement_kind, result.duration_ms
            )));
            result
        };
        if schema.is_some() {
            let _ = SqlServerEngine::use_database(&mut client, &self.database).await;
        }
        if limited.applied && result.rows.len() as u32 >= DEFAULT_QUERY_ROW_LIMIT {
            result.truncated = true;
        }
        Ok(result)
    }

    async fn close(self) -> AppResult<()> {
        self.pool.close();
        Ok(())
    }
}

fn params_from(
    host: &str,
    port: u16,
    username: &str,
    password: Option<&str>,
    database: Option<&str>,
    ssl_ca: Option<&str>,
    verify_cert: bool,
) -> AppResult<MsParams> {
    let host = host.trim();
    if host.is_empty() {
        return Err(AppError::msg("host is required"));
    }
    let username = username.trim();
    if username.is_empty() {
        return Err(AppError::msg("username is required"));
    }
    let database = database
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("master");
    validate_ident(database)?;
    Ok(MsParams {
        host: host.to_string(),
        port: if port == 0 { 1433 } else { port },
        username: username.to_string(),
        password: password.unwrap_or("").to_string(),
        database: database.to_string(),
        verify_cert,
        ca: ssl_ca
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MssqlTrust {
    /// Accept the server certificate even when it is not in the trust store.
    AcceptInvalid,
    /// Validate against the operating system trust store.
    System,
    /// Validate against the system store plus this CA file.
    Ca(String),
}

fn mssql_trust(verify_cert: bool, ca: Option<&str>) -> MssqlTrust {
    if !verify_cert {
        return MssqlTrust::AcceptInvalid;
    }
    match ca.map(str::trim).filter(|value| !value.is_empty()) {
        Some(path) => MssqlTrust::Ca(path.to_string()),
        None => MssqlTrust::System,
    }
}

fn build_config(params: &MsParams) -> Config {
    let mut config = Config::new();
    config.host(&params.host);
    config.port(params.port);
    config.database(&params.database);
    config.authentication(AuthMethod::sql_server(&params.username, &params.password));
    config.application_name("HiDataLinker");
    // SQL Server often requires TLS during prelogin. EncryptionLevel::Off still
    // upgrades to TLS, then native-tls rejects untrusted roots.
    config.encryption(EncryptionLevel::Required);
    match mssql_trust(params.verify_cert, params.ca.as_deref()) {
        MssqlTrust::AcceptInvalid => config.trust_cert(),
        MssqlTrust::System => {}
        MssqlTrust::Ca(path) => config.trust_cert_ca(path),
    }
    config
}

async fn connect_client(params: &MsParams) -> AppResult<MsClient> {
    let config = build_config(params);
    let tcp = TcpStream::connect(config.get_addr()).await?;
    tcp.set_nodelay(true)?;
    Ok(Client::connect(config, tcp.compat_write()).await?)
}

fn checked_database(name: &str) -> AppResult<String> {
    let name = name.trim();
    validate_ident(name)?;
    Ok(quote_ident_mssql(name))
}

fn nquote(value: &str) -> String {
    format!("N'{}'", value.replace('\'', "''"))
}

fn format_mssql_type(name: &str, max_length: i32, precision: u8, scale: u8) -> String {
    match name.to_ascii_lowercase().as_str() {
        "varchar" | "char" | "varbinary" | "binary" => {
            if max_length < 0 {
                format!("{name}(max)")
            } else {
                format!("{name}({max_length})")
            }
        }
        "nvarchar" | "nchar" => {
            if max_length < 0 {
                format!("{name}(max)")
            } else {
                format!("{name}({})", max_length / 2)
            }
        }
        "decimal" | "numeric" => format!("{name}({precision},{scale})"),
        "datetime2" | "datetimeoffset" | "time" => format!("{name}({scale})"),
        _ => name.to_string(),
    }
}

async fn query_rows(client: &mut MsClient, sql: &str) -> AppResult<Vec<tiberius::Row>> {
    let stream = client.simple_query(sql).await?;
    let sets = stream.into_results().await?;
    Ok(sets.into_iter().next().unwrap_or_default())
}

async fn collect_query(
    mut stream: tiberius::QueryStream<'_>,
    sql: &str,
    duration_ms: u64,
    mut messages: Vec<QueryLogEntry>,
) -> AppResult<QueryResult> {
    let mut sets: Vec<(Vec<ColumnMeta>, Vec<Vec<Option<String>>>)> = Vec::new();
    let mut columns = Vec::new();
    let mut rows = Vec::new();
    let mut truncated = false;
    let prefer_last = sql.to_ascii_uppercase().contains("STATISTICS PROFILE");

    while let Some(item) = stream.try_next().await? {
        match item {
            QueryItem::Metadata(meta) => {
                if !rows.is_empty() {
                    sets.push((std::mem::take(&mut columns), std::mem::take(&mut rows)));
                }
                columns = meta
                    .columns()
                    .iter()
                    .map(|column| ColumnMeta {
                        name: column.name().to_string(),
                        type_name: format!("{:?}", column.column_type()),
                    })
                    .collect();
            }
            QueryItem::Row(row) => {
                if rows.len() >= MAX_RESULT_ROWS {
                    truncated = true;
                    continue;
                }
                if columns.is_empty() {
                    columns = row
                        .columns()
                        .iter()
                        .map(|column| ColumnMeta {
                            name: column.name().to_string(),
                            type_name: format!("{:?}", column.column_type()),
                        })
                        .collect();
                }
                let values = (0..row.columns().len())
                    .map(|index| cell_to_string(&row, index))
                    .collect();
                rows.push(values);
            }
        }
    }
    if !columns.is_empty() || !rows.is_empty() {
        sets.push((columns, rows));
    }

    let (columns, rows) = if prefer_last {
        sets.pop().unwrap_or_default()
    } else if let Some(index) = sets.iter().position(|(_, rows)| !rows.is_empty()) {
        sets.swap_remove(index)
    } else {
        sets.pop().unwrap_or_default()
    };
    messages.push(QueryLogEntry::success(format!(
        "{} finished in {duration_ms} ms",
        statement_kind(sql)
    )));
    Ok(QueryResult {
        columns,
        rows,
        affected_rows: 0,
        last_insert_id: None,
        duration_ms,
        truncated,
        statement_kind: statement_kind(sql).to_string(),
        messages,
    })
}

async fn table_ddl(engine: &SqlServerEngine, database: &str, name: &str) -> AppResult<String> {
    let columns = engine.get_columns(database_name(database)?, name).await?;
    if columns.is_empty() {
        return Err(AppError::msg(format!("table not found: {name}")));
    }
    let (schema_name, table_name) = mssql_schema_object(name);
    let mut ddl = format!(
        "CREATE TABLE {}.{} (\n",
        quote_ident_mssql(schema_name),
        quote_ident_mssql(table_name)
    );
    for (index, column) in columns.iter().enumerate() {
        if index > 0 {
            ddl.push_str(",\n");
        }
        ddl.push_str("  ");
        ddl.push_str(&quote_ident_mssql(&column.name));
        ddl.push(' ');
        ddl.push_str(&column.column_type);
        ddl.push_str(if column.nullable {
            " NULL"
        } else {
            " NOT NULL"
        });
        if column.extra.contains("identity") {
            ddl.push_str(" IDENTITY(1,1)");
        }
        if let Some(default_value) = column.default_value.as_deref() {
            if !default_value.trim().is_empty() {
                ddl.push_str(" DEFAULT ");
                ddl.push_str(default_value.trim());
            }
        }
    }
    let keys: Vec<_> = columns
        .iter()
        .filter(|column| column.key == "PRI")
        .map(|column| quote_ident_mssql(&column.name))
        .collect();
    if !keys.is_empty() {
        ddl.push_str(",\n  PRIMARY KEY (");
        ddl.push_str(&keys.join(", "));
        ddl.push(')');
    }
    ddl.push_str("\n)");
    Ok(ddl)
}

async fn module_ddl(engine: &SqlServerEngine, database: &str, name: &str) -> AppResult<String> {
    let (schema_name, object_name) = mssql_schema_object(name);
    validate_ident(schema_name)?;
    validate_ident(object_name)?;
    let sql = format!(
        "SELECT m.definition
         FROM {database}.sys.sql_modules m
         JOIN {database}.sys.objects o ON o.object_id = m.object_id
         JOIN {database}.sys.schemas s ON s.schema_id = o.schema_id
         WHERE s.name = {} AND o.name = {}",
        nquote(schema_name),
        nquote(object_name)
    );
    let mut client = engine.client().await?;
    let rows = query_rows(&mut client, &sql).await?;
    rows.first()
        .and_then(|row| cell_to_string(row, 0))
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppError::msg(format!("definition not found: {name}")))
}

async fn index_ddl(engine: &SqlServerEngine, database: &str, name: &str) -> AppResult<String> {
    let (schema_name, table_name, index_name) = parse_index_ref(name)?;
    let sql = format!(
        "SELECT i.is_unique, i.is_primary_key, i.type_desc, c.name
         FROM {database}.sys.indexes i
         JOIN {database}.sys.tables t ON t.object_id = i.object_id
         JOIN {database}.sys.schemas s ON s.schema_id = t.schema_id
         JOIN {database}.sys.index_columns ic
           ON ic.object_id = i.object_id AND ic.index_id = i.index_id AND ic.is_included_column = 0
         JOIN {database}.sys.columns c ON c.object_id = ic.object_id AND c.column_id = ic.column_id
         WHERE s.name = {} AND t.name = {} AND i.name = {}
         ORDER BY ic.key_ordinal",
        nquote(schema_name),
        nquote(table_name),
        nquote(index_name)
    );
    let mut client = engine.client().await?;
    let rows = query_rows(&mut client, &sql).await?;
    if rows.is_empty() {
        return Err(AppError::msg(format!("index not found: {name}")));
    }
    let unique = parse_bool(cell_to_string(&rows[0], 0));
    let primary = parse_bool(cell_to_string(&rows[0], 1));
    let index_type = cell_to_string(&rows[0], 2).unwrap_or_else(|| "NONCLUSTERED".to_string());
    let columns = rows
        .iter()
        .filter_map(|row| cell_to_string(row, 3).map(|name| quote_ident_mssql(&name)))
        .collect::<Vec<_>>()
        .join(", ");
    let target = format!(
        "{}.{}",
        quote_ident_mssql(schema_name),
        quote_ident_mssql(table_name)
    );
    if primary {
        return Ok(format!(
            "ALTER TABLE {target} ADD CONSTRAINT {} PRIMARY KEY ({columns})",
            quote_ident_mssql(index_name)
        ));
    }
    let unique_sql = if unique { "UNIQUE " } else { "" };
    Ok(format!(
        "CREATE {unique_sql}{index_type} INDEX {} ON {target} ({columns})",
        quote_ident_mssql(index_name)
    ))
}

fn parse_index_ref(name: &str) -> AppResult<(&str, &str, &str)> {
    let (rest, index_name) = name
        .rsplit_once('.')
        .ok_or_else(|| AppError::msg(format!("index name must be schema.table.index: {name}")))?;
    let (schema_name, table_name) = rest
        .split_once('.')
        .ok_or_else(|| AppError::msg(format!("index name must be schema.table.index: {name}")))?;
    validate_ident(schema_name)?;
    validate_ident(table_name)?;
    validate_ident(index_name)?;
    Ok((schema_name, table_name, index_name))
}

fn database_name(quoted: &str) -> AppResult<&str> {
    let name = quoted
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(quoted);
    validate_ident(name)?;
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn certificate_verification_is_optional() {
        assert_eq!(mssql_trust(false, None), MssqlTrust::AcceptInvalid);
        assert_eq!(
            mssql_trust(false, Some("ca.pem")),
            MssqlTrust::AcceptInvalid
        );
        assert_eq!(mssql_trust(true, None), MssqlTrust::System);
        assert_eq!(mssql_trust(true, Some("  ")), MssqlTrust::System);
        assert_eq!(
            mssql_trust(true, Some("ca.pem")),
            MssqlTrust::Ca("ca.pem".to_string())
        );
    }
}
