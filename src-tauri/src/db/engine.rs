use super::dump::DumpExtras;
use super::mssql::SqlServerEngine;
use super::mysql::MySqlEngine;
use super::oracle::OracleEngine;
use super::postgres::PostgresEngine;
use crate::error::AppResult;
use crate::models::{
    CharsetCatalog, ColumnInfo, ConnectionProfile, DatabaseInfo, EngineKind, IndexInfo, ObjectKind,
    QueryResult, RoutineInfo, TableInfo, TestConnectionRequest, TriggerInfo, ViewInfo,
};
use crate::sql_audit::{AuditOutcome, AuditScope, AuditSource};
use async_trait::async_trait;
use std::time::Instant;

#[async_trait]
pub trait DatabaseEngine: Send + Sync {
    async fn ping(&self) -> AppResult<()>;
    async fn list_databases(&self) -> AppResult<Vec<DatabaseInfo>>;
    async fn create_database(
        &self,
        name: &str,
        charset: Option<&str>,
        collation: Option<&str>,
    ) -> AppResult<()>;
    async fn drop_database(&self, name: &str) -> AppResult<()>;
    async fn dump_database(
        &self,
        name: &str,
        include_schema: bool,
        include_data: bool,
    ) -> AppResult<String>;
    async fn dump_table(&self, schema: &str, table: &str) -> AppResult<String>;
    async fn list_charset_catalog(&self) -> AppResult<CharsetCatalog>;
    async fn list_tables(&self, schema: &str) -> AppResult<Vec<TableInfo>>;
    async fn list_views(&self, schema: &str) -> AppResult<Vec<ViewInfo>>;
    async fn list_indexes(&self, schema: &str) -> AppResult<Vec<IndexInfo>>;
    async fn list_triggers(&self, schema: &str) -> AppResult<Vec<TriggerInfo>>;
    async fn list_routines(&self, schema: &str) -> AppResult<Vec<RoutineInfo>>;
    async fn get_columns(&self, schema: &str, table: &str) -> AppResult<Vec<ColumnInfo>>;
    async fn get_ddl(&self, schema: &str, kind: ObjectKind, name: &str) -> AppResult<String>;
    async fn preview_table(
        &self,
        schema: &str,
        table: &str,
        limit: u32,
        offset: u64,
    ) -> AppResult<QueryResult>;
    async fn table_row_count(&self, schema: &str, table: &str) -> AppResult<u64>;
    async fn execute_sql(&self, schema: Option<&str>, sql: &str) -> AppResult<QueryResult>;
    /// Engine-specific statements a dump needs around the table/view DDL.
    async fn dump_extras(&self, _schema: &str) -> AppResult<DumpExtras> {
        Ok(DumpExtras::default())
    }
    /// Databases on the server this session could switch to. Empty when the
    /// engine does not support switching within a connection.
    async fn list_server_databases(&self) -> AppResult<Vec<String>> {
        Ok(Vec::new())
    }
    async fn close(self) -> AppResult<()>;
}

#[derive(Clone)]
enum EngineDriver {
    MySql(MySqlEngine),
    Postgres(PostgresEngine),
    SqlServer(SqlServerEngine),
    Oracle(OracleEngine),
}

/// A connected session. When `audit` is bound, every statement it runs is written to the SQL audit log.
#[derive(Clone)]
pub struct LiveEngine {
    driver: EngineDriver,
    audit: Option<AuditScope>,
}

impl LiveEngine {
    pub fn from_profile(profile: &ConnectionProfile) -> AppResult<Self> {
        let driver = match profile.engine_kind()? {
            EngineKind::MySql => EngineDriver::MySql(MySqlEngine::from_profile(profile)?),
            EngineKind::Postgres => EngineDriver::Postgres(PostgresEngine::from_profile(profile)?),
            EngineKind::SqlServer => {
                EngineDriver::SqlServer(SqlServerEngine::from_profile(profile)?)
            }
            EngineKind::Oracle => EngineDriver::Oracle(OracleEngine::from_profile(profile)?),
        };
        Ok(Self {
            driver,
            audit: None,
        })
    }

    pub fn with_audit(mut self, scope: AuditScope) -> Self {
        self.audit = Some(scope);
        self
    }

    /// Tag statements issued through this handle with the feature that sent them.
    pub fn with_source(mut self, source: AuditSource, detail: Option<String>) -> Self {
        self.audit = self
            .audit
            .take()
            .map(|scope| scope.with_source(source, detail));
        self
    }

    pub fn kind(&self) -> EngineKind {
        match &self.driver {
            EngineDriver::MySql(_) => EngineKind::MySql,
            EngineDriver::Postgres(_) => EngineKind::Postgres,
            EngineDriver::SqlServer(_) => EngineKind::SqlServer,
            EngineDriver::Oracle(_) => EngineKind::Oracle,
        }
    }

    pub async fn test(request: &TestConnectionRequest) -> AppResult<()> {
        match request.engine_kind()? {
            EngineKind::MySql => MySqlEngine::test(request).await,
            EngineKind::Postgres => PostgresEngine::test(request).await,
            EngineKind::SqlServer => SqlServerEngine::test(request).await,
            EngineKind::Oracle => OracleEngine::test(request).await,
        }
    }

    fn audit<T>(
        &self,
        operation: &str,
        schema: Option<&str>,
        sql: impl FnOnce() -> String,
        started: Instant,
        result: &AppResult<T>,
        outcome: impl FnOnce(&T) -> AuditOutcome,
    ) {
        let Some(scope) = &self.audit else {
            return;
        };
        let duration_ms = started.elapsed().as_millis() as u64;
        let result = match result {
            Ok(value) => Ok(outcome(value)),
            Err(error) => Err(error.user_message()),
        };
        scope.record(operation, schema, &sql(), duration_ms, result);
    }
}

fn query_outcome(result: &QueryResult) -> AuditOutcome {
    AuditOutcome {
        statement_kind: Some(result.statement_kind.clone()).filter(|kind| !kind.is_empty()),
        affected_rows: Some(result.affected_rows),
        row_count: Some(result.rows.len() as u64),
    }
}

fn qualified(schema: &str, name: &str) -> String {
    if schema.is_empty() {
        name.to_string()
    } else {
        format!("{schema}.{name}")
    }
}

macro_rules! dispatch_engine {
    ($self:expr, $method:ident $(, $arg:expr)* $(,)?) => {
        match &$self.driver {
            EngineDriver::MySql(engine) => engine.$method($($arg),*).await,
            EngineDriver::Postgres(engine) => engine.$method($($arg),*).await,
            EngineDriver::SqlServer(engine) => engine.$method($($arg),*).await,
            EngineDriver::Oracle(engine) => engine.$method($($arg),*).await,
        }
    };
}

#[async_trait]
impl DatabaseEngine for LiveEngine {
    async fn ping(&self) -> AppResult<()> {
        dispatch_engine!(self, ping)
    }

    async fn list_databases(&self) -> AppResult<Vec<DatabaseInfo>> {
        dispatch_engine!(self, list_databases)
    }

    async fn create_database(
        &self,
        name: &str,
        charset: Option<&str>,
        collation: Option<&str>,
    ) -> AppResult<()> {
        let started = Instant::now();
        let result = dispatch_engine!(self, create_database, name, charset, collation);
        self.audit(
            "create_database",
            None,
            || {
                let mut sql = format!("CREATE DATABASE {name}");
                if let Some(charset) = charset.filter(|value| !value.is_empty()) {
                    sql.push_str(&format!(" CHARACTER SET {charset}"));
                }
                if let Some(collation) = collation.filter(|value| !value.is_empty()) {
                    sql.push_str(&format!(" COLLATE {collation}"));
                }
                sql
            },
            started,
            &result,
            |_| AuditOutcome {
                statement_kind: Some("ddl".into()),
                ..Default::default()
            },
        );
        result
    }

    async fn drop_database(&self, name: &str) -> AppResult<()> {
        let started = Instant::now();
        let result = dispatch_engine!(self, drop_database, name);
        self.audit(
            "drop_database",
            None,
            || format!("DROP DATABASE {name}"),
            started,
            &result,
            |_| AuditOutcome {
                statement_kind: Some("ddl".into()),
                ..Default::default()
            },
        );
        result
    }

    async fn dump_database(
        &self,
        name: &str,
        include_schema: bool,
        include_data: bool,
    ) -> AppResult<String> {
        let started = Instant::now();
        let result = dispatch_engine!(self, dump_database, name, include_schema, include_data);
        self.audit(
            "dump_database",
            Some(name),
            || format!("-- dump database {name} (schema={include_schema}, data={include_data})"),
            started,
            &result,
            |_| AuditOutcome::default(),
        );
        result
    }

    async fn dump_table(&self, schema: &str, table: &str) -> AppResult<String> {
        let started = Instant::now();
        let result = dispatch_engine!(self, dump_table, schema, table);
        self.audit(
            "dump_table",
            Some(schema),
            || format!("-- dump table {}", qualified(schema, table)),
            started,
            &result,
            |_| AuditOutcome::default(),
        );
        result
    }

    async fn list_charset_catalog(&self) -> AppResult<CharsetCatalog> {
        dispatch_engine!(self, list_charset_catalog)
    }

    async fn list_tables(&self, schema: &str) -> AppResult<Vec<TableInfo>> {
        dispatch_engine!(self, list_tables, schema)
    }

    async fn list_views(&self, schema: &str) -> AppResult<Vec<ViewInfo>> {
        dispatch_engine!(self, list_views, schema)
    }

    async fn list_indexes(&self, schema: &str) -> AppResult<Vec<IndexInfo>> {
        dispatch_engine!(self, list_indexes, schema)
    }

    async fn list_triggers(&self, schema: &str) -> AppResult<Vec<TriggerInfo>> {
        dispatch_engine!(self, list_triggers, schema)
    }

    async fn list_routines(&self, schema: &str) -> AppResult<Vec<RoutineInfo>> {
        dispatch_engine!(self, list_routines, schema)
    }

    async fn get_columns(&self, schema: &str, table: &str) -> AppResult<Vec<ColumnInfo>> {
        dispatch_engine!(self, get_columns, schema, table)
    }

    async fn get_ddl(&self, schema: &str, kind: ObjectKind, name: &str) -> AppResult<String> {
        dispatch_engine!(self, get_ddl, schema, kind, name)
    }

    async fn preview_table(
        &self,
        schema: &str,
        table: &str,
        limit: u32,
        offset: u64,
    ) -> AppResult<QueryResult> {
        let started = Instant::now();
        let result = dispatch_engine!(self, preview_table, schema, table, limit, offset);
        self.audit(
            "preview_table",
            Some(schema),
            || {
                format!(
                    "SELECT * FROM {} /* limit={limit} offset={offset} */",
                    qualified(schema, table)
                )
            },
            started,
            &result,
            query_outcome,
        );
        result
    }

    async fn table_row_count(&self, schema: &str, table: &str) -> AppResult<u64> {
        let started = Instant::now();
        let result = dispatch_engine!(self, table_row_count, schema, table);
        self.audit(
            "table_row_count",
            Some(schema),
            || format!("SELECT COUNT(*) FROM {}", qualified(schema, table)),
            started,
            &result,
            |count| AuditOutcome {
                statement_kind: Some("select".into()),
                affected_rows: None,
                row_count: Some(*count),
            },
        );
        result
    }

    async fn execute_sql(&self, schema: Option<&str>, sql: &str) -> AppResult<QueryResult> {
        let started = Instant::now();
        let result = dispatch_engine!(self, execute_sql, schema, sql);
        self.audit(
            "execute_sql",
            schema,
            || sql.to_string(),
            started,
            &result,
            query_outcome,
        );
        result
    }

    async fn dump_extras(&self, schema: &str) -> AppResult<DumpExtras> {
        dispatch_engine!(self, dump_extras, schema)
    }

    async fn list_server_databases(&self) -> AppResult<Vec<String>> {
        dispatch_engine!(self, list_server_databases)
    }

    async fn close(self) -> AppResult<()> {
        match self.driver {
            EngineDriver::MySql(engine) => engine.close().await,
            EngineDriver::Postgres(engine) => engine.close().await,
            EngineDriver::SqlServer(engine) => engine.close().await,
            EngineDriver::Oracle(engine) => engine.close().await,
        }
    }
}
