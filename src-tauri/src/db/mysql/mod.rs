mod values;

use super::dump::{build_dump, build_table_data_dump, DumpDialect};
use super::engine::DatabaseEngine;
use super::ident::{
    create_mysql_database_sql, drop_mysql_database_sql, qualify, quote_ident, validate_ident,
};
use super::sql::{apply_default_query_limit, SqlDialect, DEFAULT_QUERY_ROW_LIMIT};
use crate::error::{AppError, AppResult};
use crate::models::{
    CharsetCatalog, CharsetInfo, CollationInfo, ColumnInfo, ConnectionProfile, DatabaseInfo,
    IndexInfo, ObjectKind, QueryResult, RoutineInfo, TableInfo, TestConnectionRequest, TriggerInfo,
    ViewInfo,
};
use async_trait::async_trait;
use mysql_async::prelude::*;
use mysql_async::{Conn, OptsBuilder, Pool, PoolConstraints, PoolOpts, Row, SslOpts};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;
use values::{row_to_strings, statement_kind, value_to_display};

const SYSTEM_SCHEMAS: [&str; 4] = ["information_schema", "mysql", "performance_schema", "sys"];
const MAX_RESULT_ROWS: usize = 5_000;

#[derive(Clone)]
pub struct MySqlEngine {
    pool: Pool,
}

impl MySqlEngine {
    pub fn from_profile(profile: &ConnectionProfile) -> AppResult<Self> {
        Ok(Self {
            pool: Pool::new(opts_from_profile(profile)?),
        })
    }

    pub async fn test(request: &TestConnectionRequest) -> AppResult<()> {
        let pool = Pool::new(opts_from_request(request)?);
        let mut conn = pool.get_conn().await?;
        conn.ping().await?;
        drop(conn);
        pool.disconnect().await?;
        Ok(())
    }

    async fn conn(&self) -> AppResult<Conn> {
        Ok(self.pool.get_conn().await?)
    }
}

#[async_trait]
impl DatabaseEngine for MySqlEngine {
    async fn ping(&self) -> AppResult<()> {
        let mut conn = self.conn().await?;
        conn.ping().await?;
        Ok(())
    }

    async fn list_databases(&self) -> AppResult<Vec<DatabaseInfo>> {
        let mut conn = self.conn().await?;
        let rows: Vec<(String, Option<String>, Option<String>)> = conn
            .query(
                "SELECT SCHEMA_NAME, DEFAULT_CHARACTER_SET_NAME, DEFAULT_COLLATION_NAME
                 FROM information_schema.SCHEMATA
                 ORDER BY SCHEMA_NAME",
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(|(name, charset, collation)| DatabaseInfo {
                is_system: SYSTEM_SCHEMAS.contains(&name.as_str()),
                name,
                charset,
                collation,
            })
            .collect())
    }

    async fn create_database(
        &self,
        name: &str,
        charset: Option<&str>,
        collation: Option<&str>,
    ) -> AppResult<()> {
        let sql = create_mysql_database_sql(name, charset, collation)?;
        let mut conn = self.conn().await?;
        conn.query_drop(sql).await?;
        Ok(())
    }

    async fn drop_database(&self, name: &str) -> AppResult<()> {
        let name = name.trim();
        validate_ident(name)?;
        if SYSTEM_SCHEMAS
            .iter()
            .any(|item| item.eq_ignore_ascii_case(name))
        {
            return Err(AppError::msg(format!(
                "cannot drop system database: {name}"
            )));
        }
        let sql = drop_mysql_database_sql(name)?;
        let mut conn = self.conn().await?;
        conn.query_drop(sql).await?;
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
        build_dump(self, name, include_schema, include_data, DumpDialect::Mysql).await
    }

    async fn dump_table(&self, schema: &str, table: &str) -> AppResult<String> {
        let schema = schema.trim();
        let table = table.trim();
        validate_ident(schema)?;
        validate_ident(table)?;
        build_table_data_dump(self, schema, table, DumpDialect::Mysql).await
    }

    async fn list_charset_catalog(&self) -> AppResult<CharsetCatalog> {
        let mut conn = self.conn().await?;
        let charset_rows: Vec<(String, Option<String>, Option<String>)> = conn
            .query(
                "SELECT CHARACTER_SET_NAME, DEFAULT_COLLATE_NAME, DESCRIPTION
                 FROM information_schema.CHARACTER_SETS
                 ORDER BY CHARACTER_SET_NAME",
            )
            .await?;
        let collation_rows: Vec<(String, String, String)> = conn
            .query(
                "SELECT COLLATION_NAME, CHARACTER_SET_NAME, IS_DEFAULT
                 FROM information_schema.COLLATIONS
                 ORDER BY CHARACTER_SET_NAME, COLLATION_NAME",
            )
            .await?;

        Ok(CharsetCatalog {
            charsets: charset_rows
                .into_iter()
                .map(|(name, default_collation, description)| CharsetInfo {
                    name,
                    default_collation,
                    description,
                })
                .collect(),
            collations: collation_rows
                .into_iter()
                .map(|(name, charset, is_default)| CollationInfo {
                    name,
                    charset,
                    is_default: is_default.eq_ignore_ascii_case("yes"),
                })
                .collect(),
        })
    }

    async fn list_tables(&self, schema: &str) -> AppResult<Vec<TableInfo>> {
        validate_ident(schema)?;
        let mut conn = self.conn().await?;
        let rows: Vec<(
            String,
            Option<String>,
            Option<u64>,
            Option<u64>,
            Option<String>,
            Option<String>,
            Option<String>,
        )> = conn
            .exec(
                "SELECT TABLE_NAME,
                        ENGINE,
                        TABLE_ROWS,
                        DATA_LENGTH,
                        TABLE_COMMENT,
                        DATE_FORMAT(CREATE_TIME, '%Y-%m-%d %H:%i:%s'),
                        DATE_FORMAT(UPDATE_TIME, '%Y-%m-%d %H:%i:%s')
                 FROM information_schema.TABLES
                 WHERE TABLE_SCHEMA = ? AND TABLE_TYPE = 'BASE TABLE'
                 ORDER BY TABLE_NAME",
                (schema,),
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(
                |(name, engine, table_rows, data_length, comment, created_at, updated_at)| {
                    TableInfo {
                        name,
                        engine,
                        table_rows,
                        data_length,
                        comment: comment.unwrap_or_default(),
                        created_at,
                        updated_at,
                    }
                },
            )
            .collect())
    }

    async fn list_views(&self, schema: &str) -> AppResult<Vec<ViewInfo>> {
        validate_ident(schema)?;
        let mut conn = self.conn().await?;
        let rows: Vec<(
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        )> = conn
            .exec(
                "SELECT TABLE_NAME, IS_UPDATABLE, CHECK_OPTION, SECURITY_TYPE, DEFINER
                 FROM information_schema.VIEWS
                 WHERE TABLE_SCHEMA = ?
                 ORDER BY TABLE_NAME",
                (schema,),
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(
                |(name, updatable, check_option, security_type, definer)| ViewInfo {
                    name,
                    updatable: updatable.as_deref() == Some("YES"),
                    check_option,
                    security_type,
                    definer,
                },
            )
            .collect())
    }

    async fn list_indexes(&self, schema: &str) -> AppResult<Vec<IndexInfo>> {
        validate_ident(schema)?;
        let mut conn = self.conn().await?;
        let rows: Vec<(String, String, i8, Option<String>, String, Option<String>)> = conn
            .exec(
                "SELECT INDEX_NAME, TABLE_NAME, NON_UNIQUE, INDEX_TYPE, COLUMN_NAME, INDEX_COMMENT
                 FROM information_schema.STATISTICS
                 WHERE TABLE_SCHEMA = ?
                 ORDER BY TABLE_NAME, INDEX_NAME, SEQ_IN_INDEX",
                (schema,),
            )
            .await?;

        let mut grouped: BTreeMap<(String, String), IndexInfo> = BTreeMap::new();
        for (name, table_name, non_unique, index_type, column, comment) in rows {
            let key = (table_name.clone(), name.clone());
            let entry = grouped.entry(key).or_insert_with(|| IndexInfo {
                primary: name == "PRIMARY",
                unique: non_unique == 0,
                index_type: index_type.unwrap_or_else(|| "BTREE".into()),
                comment: comment.unwrap_or_default(),
                columns: Vec::new(),
                table_name,
                name,
            });
            entry.columns.push(column);
        }
        Ok(grouped.into_values().collect())
    }

    async fn list_triggers(&self, schema: &str) -> AppResult<Vec<TriggerInfo>> {
        validate_ident(schema)?;
        let mut conn = self.conn().await?;
        let rows: Vec<(String, String, String, String, Option<String>)> = conn
            .exec(
                "SELECT TRIGGER_NAME, EVENT_OBJECT_TABLE, EVENT_MANIPULATION, ACTION_TIMING, DEFINER
                 FROM information_schema.TRIGGERS
                 WHERE TRIGGER_SCHEMA = ?
                 ORDER BY TRIGGER_NAME",
                (schema,),
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(|(name, table_name, event, timing, definer)| TriggerInfo {
                name,
                table_name,
                event,
                timing,
                definer,
            })
            .collect())
    }

    async fn list_routines(&self, schema: &str) -> AppResult<Vec<RoutineInfo>> {
        validate_ident(schema)?;
        let mut conn = self.conn().await?;
        let rows: Vec<(
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        )> = conn
            .exec(
                "SELECT ROUTINE_NAME,
                        ROUTINE_TYPE,
                        DTD_IDENTIFIER,
                        IS_DETERMINISTIC,
                        SQL_DATA_ACCESS,
                        SECURITY_TYPE,
                        DEFINER,
                        DATE_FORMAT(CREATED, '%Y-%m-%d %H:%i:%s')
                 FROM information_schema.ROUTINES
                 WHERE ROUTINE_SCHEMA = ?
                 ORDER BY ROUTINE_TYPE, ROUTINE_NAME",
                (schema,),
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(
                |(
                    name,
                    routine_type,
                    returns,
                    deterministic,
                    data_access,
                    security_type,
                    definer,
                    created_at,
                )| RoutineInfo {
                    name,
                    routine_type,
                    returns,
                    deterministic: deterministic.as_deref() == Some("YES"),
                    data_access,
                    security_type,
                    definer,
                    created_at,
                },
            )
            .collect())
    }

    async fn get_columns(&self, schema: &str, table: &str) -> AppResult<Vec<ColumnInfo>> {
        validate_ident(schema)?;
        validate_ident(table)?;
        let mut conn = self.conn().await?;
        let rows: Vec<(
            String,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            u32,
        )> = conn
            .exec(
                "SELECT COLUMN_NAME,
                        DATA_TYPE,
                        COLUMN_TYPE,
                        IS_NULLABLE,
                        COLUMN_KEY,
                        COLUMN_DEFAULT,
                        EXTRA,
                        COLUMN_COMMENT,
                        ORDINAL_POSITION
                 FROM information_schema.COLUMNS
                 WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ?
                 ORDER BY ORDINAL_POSITION",
                (schema, table),
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(
                |(
                    name,
                    data_type,
                    column_type,
                    nullable,
                    key,
                    default_value,
                    extra,
                    comment,
                    ordinal,
                )| ColumnInfo {
                    name,
                    data_type,
                    column_type,
                    nullable: nullable == "YES",
                    key: key.unwrap_or_default(),
                    default_value,
                    extra: extra.unwrap_or_default(),
                    comment: comment.unwrap_or_default(),
                    ordinal,
                },
            )
            .collect())
    }

    async fn get_ddl(&self, schema: &str, kind: ObjectKind, name: &str) -> AppResult<String> {
        validate_ident(schema)?;
        validate_ident(name)?;
        let mut conn = self.conn().await?;
        let sql = match kind {
            ObjectKind::Table | ObjectKind::View => {
                format!("SHOW CREATE TABLE {}", qualify(schema, name))
            }
            ObjectKind::Function => format!("SHOW CREATE FUNCTION {}", qualify(schema, name)),
            ObjectKind::Procedure => format!("SHOW CREATE PROCEDURE {}", qualify(schema, name)),
            ObjectKind::Trigger => format!("SHOW CREATE TRIGGER {}", qualify(schema, name)),
            ObjectKind::Index => {
                return Err(AppError::msg(
                    "indexes do not have standalone CREATE statements; inspect the parent table DDL",
                ));
            }
        };

        let row: Option<Row> = conn.query_first(sql).await?;
        let row = row.ok_or_else(|| AppError::msg("object DDL was empty"))?;
        extract_ddl(&row)
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
            "SELECT * FROM {} LIMIT {limit} OFFSET {offset}",
            qualify(schema, table)
        );
        self.execute_sql(Some(schema), &sql).await
    }

    async fn table_row_count(&self, schema: &str, table: &str) -> AppResult<u64> {
        validate_ident(schema)?;
        validate_ident(table)?;
        let mut conn = self.conn().await?;
        let sql = format!("SELECT COUNT(*) FROM {}", qualify(schema, table));
        let count: Option<u64> = conn.query_first(sql).await?;
        Ok(count.unwrap_or(0))
    }

    async fn execute_sql(&self, schema: Option<&str>, sql: &str) -> AppResult<QueryResult> {
        let sql = sql.trim();
        if sql.is_empty() {
            return Err(AppError::msg("SQL is empty"));
        }
        let limited = apply_default_query_limit(sql, SqlDialect::MySql);
        let sql = limited.sql.as_str();

        let mut messages = Vec::new();
        let mut conn = self.conn().await?;
        if let Some(schema) = schema {
            validate_ident(schema)?;
            conn.query_drop(format!("USE {}", quote_ident(schema)))
                .await?;
            messages.push(crate::models::QueryLogEntry::info(format!("USE {schema}")));
        }

        if limited.applied {
            messages.push(crate::models::QueryLogEntry::info(format!(
                "No LIMIT specified; applying default LIMIT {DEFAULT_QUERY_ROW_LIMIT}"
            )));
        }
        messages.push(crate::models::QueryLogEntry::info(format!(
            "Executing {}…",
            statement_kind(sql)
        )));

        let started = Instant::now();
        let mut result = conn.query_iter(sql).await?;
        let columns: Vec<crate::models::ColumnMeta> = result
            .columns_ref()
            .iter()
            .map(|column| crate::models::ColumnMeta {
                name: column.name_str().into_owned(),
                type_name: format!("{:?}", column.column_type()),
            })
            .collect();

        let mut rows = Vec::new();
        let mut truncated = false;
        while let Some(row) = result.next().await? {
            if rows.len() >= MAX_RESULT_ROWS {
                truncated = true;
                break;
            }
            rows.push(row_to_strings(&row));
        }

        let affected_rows = result.affected_rows();
        let last_insert_id = result.last_insert_id();
        let info = result.info().trim().to_string();
        let warning_count = result.warnings();
        let duration_ms = started.elapsed().as_millis() as u64;

        if columns.is_empty() {
            messages.push(crate::models::QueryLogEntry::success(format!(
                "OK, {affected_rows} row(s) affected"
            )));
        } else {
            messages.push(crate::models::QueryLogEntry::success(format!(
                "OK, {} column(s), {} row(s) returned",
                columns.len(),
                rows.len()
            )));
        }
        if !info.is_empty() {
            messages.push(crate::models::QueryLogEntry::notice(info));
        }
        if let Some(id) = last_insert_id {
            messages.push(crate::models::QueryLogEntry::info(format!(
                "Last insert id: {id}"
            )));
        }
        if !truncated && limited.applied && rows.len() >= DEFAULT_QUERY_ROW_LIMIT as usize {
            truncated = true;
        }
        if truncated {
            let cap = if limited.applied {
                DEFAULT_QUERY_ROW_LIMIT as usize
            } else {
                MAX_RESULT_ROWS
            };
            messages.push(crate::models::QueryLogEntry::warning(format!(
                "Result truncated to {cap} rows"
            )));
        }
        if warning_count > 0 {
            messages.push(crate::models::QueryLogEntry::warning(format!(
                "{warning_count} warning(s)"
            )));
            // Consume remaining result sets before issuing SHOW WARNINGS.
            let _ = result.drop_result().await;
            append_mysql_warnings(&mut conn, &mut messages).await?;
        } else {
            let _ = result.drop_result().await;
        }

        messages.push(crate::models::QueryLogEntry::info(format!(
            "Finished in {duration_ms} ms"
        )));

        Ok(QueryResult {
            columns,
            rows,
            affected_rows,
            last_insert_id,
            duration_ms,
            truncated,
            statement_kind: statement_kind(sql).to_string(),
            messages,
        })
    }

    async fn close(self) -> AppResult<()> {
        self.pool.disconnect().await?;
        Ok(())
    }
}

fn opts_from_profile(profile: &ConnectionProfile) -> AppResult<OptsBuilder> {
    build_opts(
        &profile.host,
        profile.port,
        &profile.username,
        profile.password.as_deref(),
        profile.database.as_deref(),
        profile.ssl_ca.as_deref(),
    )
}

fn opts_from_request(request: &TestConnectionRequest) -> AppResult<OptsBuilder> {
    build_opts(
        &request.host,
        request.port,
        &request.username,
        request.password.as_deref(),
        request.database.as_deref(),
        request.ssl_ca.as_deref(),
    )
}

fn build_opts(
    host: &str,
    port: u16,
    username: &str,
    password: Option<&str>,
    database: Option<&str>,
    ssl_ca: Option<&str>,
) -> AppResult<OptsBuilder> {
    let constraints = PoolConstraints::new(0, 16).expect("valid pool constraints");
    let mut opts = OptsBuilder::default()
        .ip_or_hostname(host)
        .tcp_port(port)
        .user(Some(username))
        .pass(password)
        .pool_opts(PoolOpts::new().with_constraints(constraints))
        .prefer_socket(false)
        .stmt_cache_size(16);

    if let Some(database) = database.filter(|value| !value.is_empty()) {
        opts = opts.db_name(Some(database));
    }
    if let Some(ssl_opts) = ssl_opts_from_ca(ssl_ca)? {
        opts = opts.ssl_opts(ssl_opts);
    }
    Ok(opts)
}

fn ssl_opts_from_ca(ssl_ca: Option<&str>) -> AppResult<Option<SslOpts>> {
    let Some(ca) = ssl_ca.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let path = PathBuf::from(ca);
    if !path.is_file() {
        return Err(AppError::msg(format!("CA certificate not found: {ca}")));
    }
    Ok(Some(SslOpts::default().with_root_certs(vec![path.into()])))
}

fn extract_ddl(row: &Row) -> AppResult<String> {
    for index in 0..row.len() {
        if let Some(value) = row.as_ref(index) {
            if let Some(text) = value_to_display(value) {
                let upper = text.trim_start().to_ascii_uppercase();
                if upper.starts_with("CREATE ") || upper.starts_with("ALTER ") {
                    return Ok(text);
                }
            }
        }
    }
    if row.len() >= 2 {
        if let Some(value) = row.as_ref(1).and_then(value_to_display) {
            return Ok(value);
        }
    }
    Err(AppError::msg("unable to read CREATE statement"))
}

async fn append_mysql_warnings(
    conn: &mut Conn,
    messages: &mut Vec<crate::models::QueryLogEntry>,
) -> AppResult<()> {
    let warnings: Vec<(String, u16, String)> = conn
        .query_map("SHOW WARNINGS", |row: (String, u16, String)| row)
        .await
        .unwrap_or_default();
    for (level, code, text) in warnings.into_iter().take(20) {
        let entry = format!("[{level}] ({code}) {text}");
        if level.eq_ignore_ascii_case("Error") {
            messages.push(crate::models::QueryLogEntry::error(entry));
        } else if level.eq_ignore_ascii_case("Note") {
            messages.push(crate::models::QueryLogEntry::notice(entry));
        } else {
            messages.push(crate::models::QueryLogEntry::warning(entry));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_ssl_when_ca_missing() {
        assert!(ssl_opts_from_ca(None).unwrap().is_none());
        assert!(ssl_opts_from_ca(Some("")).unwrap().is_none());
        assert!(ssl_opts_from_ca(Some("  ")).unwrap().is_none());
    }

    #[test]
    fn rejects_missing_ca_file() {
        let error = ssl_opts_from_ca(Some("/definitely/missing/ca.pem")).unwrap_err();
        assert!(error.to_string().contains("CA certificate not found"));
    }
}
