use super::mssql::SqlServerEngine;
use super::mysql::MySqlEngine;
use super::oracle::OracleEngine;
use super::postgres::PostgresEngine;
use crate::error::AppResult;
use crate::models::{
    CharsetCatalog, ColumnInfo, ConnectionProfile, DatabaseInfo, EngineKind, IndexInfo, ObjectKind,
    QueryResult, RoutineInfo, TableInfo, TestConnectionRequest, TriggerInfo, ViewInfo,
};
use async_trait::async_trait;

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
    async fn close(self) -> AppResult<()>;
}

#[derive(Clone)]
pub enum LiveEngine {
    MySql(MySqlEngine),
    Postgres(PostgresEngine),
    SqlServer(SqlServerEngine),
    Oracle(OracleEngine),
}

impl LiveEngine {
    pub fn from_profile(profile: &ConnectionProfile) -> AppResult<Self> {
        match profile.engine_kind()? {
            EngineKind::MySql => Ok(Self::MySql(MySqlEngine::from_profile(profile)?)),
            EngineKind::Postgres => Ok(Self::Postgres(PostgresEngine::from_profile(profile)?)),
            EngineKind::SqlServer => Ok(Self::SqlServer(SqlServerEngine::from_profile(profile)?)),
            EngineKind::Oracle => Ok(Self::Oracle(OracleEngine::from_profile(profile)?)),
        }
    }

    pub fn kind(&self) -> EngineKind {
        match self {
            Self::MySql(_) => EngineKind::MySql,
            Self::Postgres(_) => EngineKind::Postgres,
            Self::SqlServer(_) => EngineKind::SqlServer,
            Self::Oracle(_) => EngineKind::Oracle,
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
}

macro_rules! dispatch_engine {
    ($self:expr, $method:ident $(, $arg:expr)* $(,)?) => {
        match $self {
            Self::MySql(engine) => engine.$method($($arg),*).await,
            Self::Postgres(engine) => engine.$method($($arg),*).await,
            Self::SqlServer(engine) => engine.$method($($arg),*).await,
            Self::Oracle(engine) => engine.$method($($arg),*).await,
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
        dispatch_engine!(self, create_database, name, charset, collation)
    }

    async fn drop_database(&self, name: &str) -> AppResult<()> {
        dispatch_engine!(self, drop_database, name)
    }

    async fn dump_database(
        &self,
        name: &str,
        include_schema: bool,
        include_data: bool,
    ) -> AppResult<String> {
        dispatch_engine!(self, dump_database, name, include_schema, include_data)
    }

    async fn dump_table(&self, schema: &str, table: &str) -> AppResult<String> {
        dispatch_engine!(self, dump_table, schema, table)
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
        dispatch_engine!(self, preview_table, schema, table, limit, offset)
    }

    async fn table_row_count(&self, schema: &str, table: &str) -> AppResult<u64> {
        dispatch_engine!(self, table_row_count, schema, table)
    }

    async fn execute_sql(&self, schema: Option<&str>, sql: &str) -> AppResult<QueryResult> {
        dispatch_engine!(self, execute_sql, schema, sql)
    }

    async fn close(self) -> AppResult<()> {
        dispatch_engine!(self, close)
    }
}
