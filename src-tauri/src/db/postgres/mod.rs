use super::dump::{build_dump, build_table_data_dump, DumpDialect};
use super::engine::DatabaseEngine;
use super::ident::{
    create_pg_schema_sql, drop_pg_schema_sql, qualify_pg as qualify, quote_ident_pg as quote_ident,
    validate_ident,
};
use super::sql::{apply_default_query_limit, statement_kind, SqlDialect, DEFAULT_QUERY_ROW_LIMIT};
use crate::error::{AppError, AppResult};
use crate::models::{
    CharsetCatalog, ColumnInfo, ConnectionProfile, DatabaseInfo, IndexInfo, ObjectKind,
    QueryResult, RoutineInfo, TableInfo, TestConnectionRequest, TriggerInfo, ViewInfo,
};
use async_trait::async_trait;
use deadpool_postgres::{Config, Pool, PoolConfig, Runtime};
use std::collections::BTreeMap;
use std::time::Instant;
use tokio_postgres::{NoTls, SimpleQueryMessage};

const SYSTEM_SCHEMAS: [&str; 3] = ["pg_catalog", "information_schema", "pg_toast"];
const MAX_RESULT_ROWS: usize = 5_000;

#[derive(Clone)]
pub struct PostgresEngine {
    pool: Pool,
}

impl PostgresEngine {
    pub fn from_profile(profile: &ConnectionProfile) -> AppResult<Self> {
        Ok(Self {
            pool: create_pool(
                &profile.host,
                profile.port,
                &profile.username,
                profile.password.as_deref(),
                profile.database.as_deref(),
            )?,
        })
    }

    pub async fn test(request: &TestConnectionRequest) -> AppResult<()> {
        let pool = create_pool(
            &request.host,
            request.port,
            &request.username,
            request.password.as_deref(),
            request.database.as_deref(),
        )?;
        let client = pool.get().await?;
        client.simple_query("SELECT 1").await?;
        drop(client);
        pool.close();
        Ok(())
    }

    async fn set_search_path(
        client: &deadpool_postgres::Client,
        schema: Option<&str>,
    ) -> AppResult<()> {
        if let Some(schema) = schema {
            validate_ident(schema)?;
            let sql = format!("SET search_path TO {}, pg_catalog", quote_ident(schema));
            client.simple_query(&sql).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl DatabaseEngine for PostgresEngine {
    async fn ping(&self) -> AppResult<()> {
        let client = self.pool.get().await?;
        client.simple_query("SELECT 1").await?;
        Ok(())
    }

    async fn list_databases(&self) -> AppResult<Vec<DatabaseInfo>> {
        let client = self.pool.get().await?;
        let rows = client
            .query(
                "SELECT n.nspname,
                        pg_catalog.pg_encoding_to_char(d.encoding),
                        d.datcollate
                 FROM pg_namespace n
                 CROSS JOIN pg_database d
                 WHERE d.datname = current_database()
                   AND n.nspname NOT LIKE 'pg_toast%'
                   AND n.nspname NOT LIKE 'pg_temp%'
                   AND n.nspname NOT LIKE 'pg_toast_temp%'
                 ORDER BY n.nspname",
                &[],
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let name: String = row.get(0);
                let charset: Option<String> = row.get(1);
                let collation: Option<String> = row.get(2);
                DatabaseInfo {
                    is_system: SYSTEM_SCHEMAS.contains(&name.as_str())
                        || name.starts_with("pg_toast")
                        || name.starts_with("pg_temp"),
                    name,
                    charset,
                    collation,
                }
            })
            .collect())
    }

    async fn create_database(
        &self,
        name: &str,
        _charset: Option<&str>,
        _collation: Option<&str>,
    ) -> AppResult<()> {
        let sql = create_pg_schema_sql(name)?;
        let client = self.pool.get().await?;
        client.simple_query(&sql).await?;
        Ok(())
    }

    async fn drop_database(&self, name: &str) -> AppResult<()> {
        let name = name.trim();
        validate_ident(name)?;
        if SYSTEM_SCHEMAS
            .iter()
            .any(|item| item.eq_ignore_ascii_case(name))
            || name.eq_ignore_ascii_case("public")
            || name.starts_with("pg_")
        {
            return Err(AppError::msg(format!("cannot drop system schema: {name}")));
        }
        let sql = drop_pg_schema_sql(name)?;
        let client = self.pool.get().await?;
        client.simple_query(&sql).await?;
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
        build_dump(
            self,
            name,
            include_schema,
            include_data,
            DumpDialect::Postgres,
        )
        .await
    }

    async fn dump_table(&self, schema: &str, table: &str) -> AppResult<String> {
        let schema = schema.trim();
        let table = table.trim();
        validate_ident(schema)?;
        validate_ident(table)?;
        build_table_data_dump(self, schema, table, DumpDialect::Postgres).await
    }

    async fn list_charset_catalog(&self) -> AppResult<CharsetCatalog> {
        Ok(CharsetCatalog {
            charsets: Vec::new(),
            collations: Vec::new(),
        })
    }

    async fn list_tables(&self, schema: &str) -> AppResult<Vec<TableInfo>> {
        validate_ident(schema)?;
        let client = self.pool.get().await?;
        let rows = client
            .query(
                "SELECT c.relname,
                        COALESCE(am.amname, 'heap'),
                        GREATEST(c.reltuples, 0)::bigint,
                        pg_total_relation_size(c.oid),
                        COALESCE(obj_description(c.oid, 'pg_class'), '')
                 FROM pg_class c
                 JOIN pg_namespace n ON n.oid = c.relnamespace
                 LEFT JOIN pg_am am ON am.oid = c.relam
                 WHERE n.nspname = $1
                   AND c.relkind IN ('r', 'p', 'f')
                 ORDER BY c.relname",
                &[&schema],
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| TableInfo {
                name: row.get(0),
                engine: row.get(1),
                table_rows: {
                    let value: i64 = row.get(2);
                    Some(value.max(0) as u64)
                },
                data_length: {
                    let value: i64 = row.get(3);
                    Some(value.max(0) as u64)
                },
                comment: row.get(4),
                created_at: None,
                updated_at: None,
            })
            .collect())
    }

    async fn list_views(&self, schema: &str) -> AppResult<Vec<ViewInfo>> {
        validate_ident(schema)?;
        let client = self.pool.get().await?;
        let rows = client
            .query(
                "SELECT c.relname,
                        c.relkind = 'v',
                        CASE c.relkind WHEN 'm' THEN 'MATERIALIZED' ELSE 'VIEW' END,
                        pg_catalog.pg_get_userbyid(c.relowner)
                 FROM pg_class c
                 JOIN pg_namespace n ON n.oid = c.relnamespace
                 WHERE n.nspname = $1
                   AND c.relkind IN ('v', 'm')
                 ORDER BY c.relname",
                &[&schema],
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let materialized: Option<String> = row.get(2);
                ViewInfo {
                    name: row.get(0),
                    updatable: row.get(1),
                    check_option: materialized,
                    security_type: None,
                    definer: row.get(3),
                }
            })
            .collect())
    }

    async fn list_indexes(&self, schema: &str) -> AppResult<Vec<IndexInfo>> {
        validate_ident(schema)?;
        let client = self.pool.get().await?;
        let rows = client
            .query(
                "SELECT ic.relname,
                        t.relname,
                        i.indisunique,
                        i.indisprimary,
                        am.amname,
                        COALESCE(
                            a.attname,
                            pg_get_indexdef(i.indexrelid, cols.ordinality::int, true)
                        ),
                        COALESCE(obj_description(ic.oid, 'pg_class'), '')
                 FROM pg_index i
                 JOIN pg_class ic ON ic.oid = i.indexrelid
                 JOIN pg_class t ON t.oid = i.indrelid
                 JOIN pg_namespace n ON n.oid = t.relnamespace
                 JOIN pg_am am ON am.oid = ic.relam
                 JOIN LATERAL unnest(i.indkey) WITH ORDINALITY AS cols(attnum, ordinality) ON true
                 LEFT JOIN pg_attribute a
                   ON a.attrelid = t.oid AND a.attnum = cols.attnum
                 WHERE n.nspname = $1
                 ORDER BY t.relname, ic.relname, cols.ordinality",
                &[&schema],
            )
            .await?;

        let mut grouped: BTreeMap<(String, String), IndexInfo> = BTreeMap::new();
        for row in rows {
            let name: String = row.get(0);
            let table_name: String = row.get(1);
            let unique: bool = row.get(2);
            let primary: bool = row.get(3);
            let index_type: String = row.get(4);
            let column: Option<String> = row.get(5);
            let comment: String = row.get(6);
            let key = (table_name.clone(), name.clone());
            let entry = grouped.entry(key).or_insert_with(|| IndexInfo {
                primary,
                unique,
                index_type,
                comment,
                columns: Vec::new(),
                table_name,
                name,
            });
            if let Some(column) = column {
                entry.columns.push(column);
            }
        }
        Ok(grouped.into_values().collect())
    }

    async fn list_triggers(&self, schema: &str) -> AppResult<Vec<TriggerInfo>> {
        validate_ident(schema)?;
        let client = self.pool.get().await?;
        let rows = client
            .query(
                "SELECT t.tgname,
                        c.relname,
                        concat_ws(' OR ',
                            CASE WHEN t.tgtype & 4 <> 0 THEN 'INSERT' END,
                            CASE WHEN t.tgtype & 8 <> 0 THEN 'DELETE' END,
                            CASE WHEN t.tgtype & 16 <> 0 THEN 'UPDATE' END,
                            CASE WHEN t.tgtype & 32 <> 0 THEN 'TRUNCATE' END
                        ),
                        CASE
                            WHEN t.tgtype & 2 <> 0 THEN 'BEFORE'
                            WHEN t.tgtype & 64 <> 0 THEN 'INSTEAD OF'
                            ELSE 'AFTER'
                        END,
                        pg_catalog.pg_get_userbyid(c.relowner)
                 FROM pg_trigger t
                 JOIN pg_class c ON c.oid = t.tgrelid
                 JOIN pg_namespace n ON n.oid = c.relnamespace
                 WHERE n.nspname = $1
                   AND NOT t.tgisinternal
                 ORDER BY t.tgname",
                &[&schema],
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| TriggerInfo {
                name: row.get(0),
                table_name: row.get(1),
                event: row.get(2),
                timing: row.get(3),
                definer: row.get(4),
            })
            .collect())
    }

    async fn list_routines(&self, schema: &str) -> AppResult<Vec<RoutineInfo>> {
        validate_ident(schema)?;
        let client = self.pool.get().await?;
        let rows = client
            .query(
                "SELECT CASE
                            WHEN pg_get_function_identity_arguments(p.oid) = '' THEN p.proname
                            ELSE p.proname || '(' || pg_get_function_identity_arguments(p.oid) || ')'
                        END,
                        CASE p.prokind
                            WHEN 'p' THEN 'PROCEDURE'
                            ELSE 'FUNCTION'
                        END,
                        pg_get_function_result(p.oid),
                        p.provolatile = 'i',
                        CASE p.provolatile
                            WHEN 'i' THEN 'IMMUTABLE'
                            WHEN 's' THEN 'STABLE'
                            ELSE 'VOLATILE'
                        END,
                        CASE p.prosecdef WHEN true THEN 'DEFINER' ELSE 'INVOKER' END,
                        pg_catalog.pg_get_userbyid(p.proowner)
                 FROM pg_proc p
                 JOIN pg_namespace n ON n.oid = p.pronamespace
                 WHERE n.nspname = $1
                   AND p.prokind IN ('f', 'p')
                 ORDER BY p.prokind, p.proname, p.oid",
                &[&schema],
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| RoutineInfo {
                name: row.get(0),
                routine_type: row.get(1),
                returns: row.get(2),
                deterministic: row.get(3),
                data_access: row.get(4),
                security_type: row.get(5),
                definer: row.get(6),
                created_at: None,
            })
            .collect())
    }

    async fn get_columns(&self, schema: &str, table: &str) -> AppResult<Vec<ColumnInfo>> {
        validate_ident(schema)?;
        validate_ident(table)?;
        let client = self.pool.get().await?;
        let rows = client
            .query(
                "SELECT a.attname,
                        pg_catalog.format_type(a.atttypid, a.atttypmod),
                        NOT a.attnotnull,
                        CASE
                            WHEN EXISTS (
                                SELECT 1 FROM pg_index i
                                WHERE i.indrelid = c.oid
                                  AND i.indisprimary
                                  AND a.attnum = ANY (i.indkey)
                            ) THEN 'PRI'
                            WHEN EXISTS (
                                SELECT 1 FROM pg_index i
                                WHERE i.indrelid = c.oid
                                  AND i.indisunique
                                  AND a.attnum = ANY (i.indkey)
                            ) THEN 'UNI'
                            ELSE ''
                        END,
                        pg_get_expr(ad.adbin, ad.adrelid),
                        CASE
                            WHEN a.attidentity = 'a' THEN 'GENERATED ALWAYS AS IDENTITY'
                            WHEN a.attidentity = 'd' THEN 'GENERATED BY DEFAULT AS IDENTITY'
                            WHEN a.attgenerated = 's' THEN 'GENERATED ALWAYS AS STORED'
                            ELSE ''
                        END,
                        COALESCE(col_description(c.oid, a.attnum), ''),
                        a.attnum
                 FROM pg_attribute a
                 JOIN pg_class c ON c.oid = a.attrelid
                 JOIN pg_namespace n ON n.oid = c.relnamespace
                 LEFT JOIN pg_attrdef ad
                   ON ad.adrelid = a.attrelid AND ad.adnum = a.attnum
                 WHERE n.nspname = $1
                   AND c.relname = $2
                   AND a.attnum > 0
                   AND NOT a.attisdropped
                 ORDER BY a.attnum",
                &[&schema, &table],
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let data_type: String = row.get(1);
                let attnum: i16 = row.get(7);
                ColumnInfo {
                    name: row.get(0),
                    column_type: data_type.clone(),
                    data_type,
                    nullable: row.get(2),
                    key: row.get(3),
                    default_value: row.get(4),
                    extra: row.get(5),
                    comment: row.get(6),
                    ordinal: attnum.max(0) as u32,
                }
            })
            .collect())
    }

    async fn get_ddl(&self, schema: &str, kind: ObjectKind, name: &str) -> AppResult<String> {
        validate_ident(schema)?;
        match kind {
            ObjectKind::Table => table_ddl(&self.pool, schema, name).await,
            ObjectKind::View => view_ddl(&self.pool, schema, name).await,
            ObjectKind::Function | ObjectKind::Procedure => {
                validate_ident(bare_routine_name(name))?;
                function_ddl(&self.pool, schema, name).await
            }
            ObjectKind::Trigger => {
                validate_ident(name)?;
                trigger_ddl(&self.pool, schema, name).await
            }
            ObjectKind::Index => {
                validate_ident(name)?;
                index_ddl(&self.pool, schema, name).await
            }
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
            "SELECT * FROM {} LIMIT {limit} OFFSET {offset}",
            qualify(schema, table)
        );
        self.execute_sql(Some(schema), &sql).await
    }

    async fn table_row_count(&self, schema: &str, table: &str) -> AppResult<u64> {
        validate_ident(schema)?;
        validate_ident(table)?;
        let client = self.pool.get().await?;
        let sql = format!("SELECT COUNT(*)::bigint FROM {}", qualify(schema, table));
        let row = client.query_one(&sql, &[]).await?;
        let count: i64 = row.get(0);
        Ok(count.max(0) as u64)
    }

    async fn execute_sql(&self, schema: Option<&str>, sql: &str) -> AppResult<QueryResult> {
        let sql = sql.trim();
        if sql.is_empty() {
            return Err(AppError::msg("SQL is empty"));
        }
        let limited = apply_default_query_limit(sql, SqlDialect::Postgres);
        let sql = limited.sql.as_str();

        let mut messages = Vec::new();
        let client = self.pool.get().await?;
        Self::set_search_path(&client, schema).await?;
        if let Some(schema) = schema {
            messages.push(crate::models::QueryLogEntry::info(format!(
                "SET search_path TO {schema}, pg_catalog"
            )));
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
        let result = client.simple_query(sql).await;
        if schema.is_some() {
            let _ = client.simple_query("RESET search_path").await;
        }
        let query_messages = result?;
        Ok(simple_query_result(
            query_messages,
            sql,
            started.elapsed().as_millis() as u64,
            messages,
            limited.applied,
        ))
    }

    async fn close(self) -> AppResult<()> {
        self.pool.close();
        Ok(())
    }
}

fn create_pool(
    host: &str,
    port: u16,
    username: &str,
    password: Option<&str>,
    database: Option<&str>,
) -> AppResult<Pool> {
    let mut cfg = Config::new();
    cfg.host = Some(host.to_string());
    cfg.port = Some(if port == 0 { 5432 } else { port });
    cfg.user = Some(username.to_string());
    cfg.password = password
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let dbname = database
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("postgres");
    cfg.dbname = Some(dbname.to_string());
    cfg.pool = Some(PoolConfig::new(16));
    Ok(cfg.create_pool(Some(Runtime::Tokio1), NoTls)?)
}

fn simple_query_result(
    messages: Vec<SimpleQueryMessage>,
    sql: &str,
    duration_ms: u64,
    mut log: Vec<crate::models::QueryLogEntry>,
    default_limit_applied: bool,
) -> QueryResult {
    let mut columns = Vec::new();
    let mut rows = Vec::new();
    let mut current_columns = Vec::new();
    let mut current_rows = Vec::new();
    let mut affected_rows = 0_u64;
    let mut truncated = false;
    let mut statement_index = 0_u32;

    for message in messages {
        match message {
            SimpleQueryMessage::Row(row) => {
                if current_columns.is_empty() {
                    current_columns = row
                        .columns()
                        .iter()
                        .map(|column| crate::models::ColumnMeta {
                            name: column.name().to_string(),
                            type_name: String::new(),
                        })
                        .collect();
                }
                if current_rows.len() >= MAX_RESULT_ROWS {
                    truncated = true;
                } else {
                    current_rows.push(
                        (0..row.len())
                            .map(|index| row.get(index).map(str::to_string))
                            .collect(),
                    );
                }
            }
            SimpleQueryMessage::CommandComplete(count) => {
                statement_index += 1;
                affected_rows = count;
                if !current_columns.is_empty() {
                    let row_count = current_rows.len();
                    let col_count = current_columns.len();
                    columns = std::mem::take(&mut current_columns);
                    rows = std::mem::take(&mut current_rows);
                    log.push(crate::models::QueryLogEntry::success(format!(
                        "Statement #{statement_index}: OK, {col_count} column(s), {row_count} row(s) returned"
                    )));
                } else {
                    log.push(crate::models::QueryLogEntry::success(format!(
                        "Statement #{statement_index}: OK, {count} row(s) affected"
                    )));
                }
            }
            _ => {}
        }
    }

    if !current_columns.is_empty() {
        columns = current_columns;
        rows = current_rows;
    }

    if !truncated && default_limit_applied && rows.len() >= DEFAULT_QUERY_ROW_LIMIT as usize {
        truncated = true;
    }
    if truncated {
        let cap = if default_limit_applied {
            DEFAULT_QUERY_ROW_LIMIT as usize
        } else {
            MAX_RESULT_ROWS
        };
        log.push(crate::models::QueryLogEntry::warning(format!(
            "Result truncated to {cap} rows"
        )));
    }
    if statement_index == 0 && columns.is_empty() {
        log.push(crate::models::QueryLogEntry::success("OK".to_string()));
    }
    log.push(crate::models::QueryLogEntry::info(format!(
        "Finished in {duration_ms} ms"
    )));

    QueryResult {
        columns,
        rows,
        affected_rows,
        last_insert_id: None,
        duration_ms,
        truncated,
        statement_kind: statement_kind(sql).to_string(),
        messages: log,
    }
}

fn bare_routine_name(name: &str) -> &str {
    name.split('(').next().unwrap_or(name)
}

async fn table_ddl(pool: &Pool, schema: &str, name: &str) -> AppResult<String> {
    validate_ident(name)?;
    let client = pool.get().await?;
    let columns = client
        .query(
            "SELECT a.attname,
                    pg_catalog.format_type(a.atttypid, a.atttypmod),
                    a.attnotnull,
                    pg_get_expr(ad.adbin, ad.adrelid),
                    CASE a.attidentity WHEN 'a' THEN 'a' WHEN 'd' THEN 'd' ELSE '' END,
                    CASE WHEN a.attgenerated = 's' THEN pg_get_expr(ad.adbin, ad.adrelid) ELSE NULL END,
                    col_description(c.oid, a.attnum)
             FROM pg_attribute a
             JOIN pg_class c ON c.oid = a.attrelid
             JOIN pg_namespace n ON n.oid = c.relnamespace
             LEFT JOIN pg_attrdef ad
               ON ad.adrelid = a.attrelid AND ad.adnum = a.attnum
             WHERE n.nspname = $1
               AND c.relname = $2
               AND a.attnum > 0
               AND NOT a.attisdropped
             ORDER BY a.attnum",
            &[&schema, &name],
        )
        .await?;

    if columns.is_empty() {
        return Err(AppError::msg("table was not found"));
    }

    let mut lines = Vec::new();
    for row in columns {
        let col_name: String = row.get(0);
        let type_name: String = row.get(1);
        let not_null: bool = row.get(2);
        let default_value: Option<String> = row.get(3);
        let identity: String = row.get(4);
        let generated: Option<String> = row.get(5);
        let comment: Option<String> = row.get(6);
        let mut line = format!("    {} {}", quote_ident(&col_name), type_name);
        match identity.as_str() {
            "a" => line.push_str(" GENERATED ALWAYS AS IDENTITY"),
            "d" => line.push_str(" GENERATED BY DEFAULT AS IDENTITY"),
            _ => {
                if let Some(expr) = generated {
                    line.push_str(&format!(" GENERATED ALWAYS AS ({expr}) STORED"));
                } else if let Some(default_value) = default_value {
                    line.push_str(&format!(" DEFAULT {default_value}"));
                }
            }
        }
        if not_null && identity.is_empty() {
            line.push_str(" NOT NULL");
        }
        if let Some(comment) = comment.filter(|value| !value.is_empty()) {
            line.push_str(&format!(" -- {comment}"));
        }
        lines.push(line);
    }

    let constraints = client
        .query(
            "SELECT pg_get_constraintdef(co.oid, true)
             FROM pg_constraint co
             JOIN pg_class c ON c.oid = co.conrelid
             JOIN pg_namespace n ON n.oid = c.relnamespace
             WHERE n.nspname = $1
               AND c.relname = $2
               AND co.conparentid = 0
             ORDER BY CASE co.contype
                        WHEN 'p' THEN 0
                        WHEN 'u' THEN 1
                        WHEN 'c' THEN 2
                        WHEN 'f' THEN 3
                        ELSE 4
                      END,
                      co.conname",
            &[&schema, &name],
        )
        .await?;
    for row in constraints {
        let def: String = row.get(0);
        lines.push(format!("    {def}"));
    }

    let mut ddl = format!(
        "CREATE TABLE {} (\n{}\n);",
        qualify(schema, name),
        lines.join(",\n")
    );

    let indexes = client
        .query(
            "SELECT pg_get_indexdef(i.indexrelid)
             FROM pg_index i
             JOIN pg_class t ON t.oid = i.indrelid
             JOIN pg_namespace n ON n.oid = t.relnamespace
             WHERE n.nspname = $1
               AND t.relname = $2
               AND NOT i.indisprimary
               AND NOT EXISTS (
                    SELECT 1 FROM pg_constraint co WHERE co.conindid = i.indexrelid
               )
             ORDER BY i.indexrelid",
            &[&schema, &name],
        )
        .await?;
    for row in indexes {
        let def: String = row.get(0);
        ddl.push_str("\n\n");
        ddl.push_str(&def);
        if !def.ends_with(';') {
            ddl.push(';');
        }
    }

    Ok(ddl)
}

async fn view_ddl(pool: &Pool, schema: &str, name: &str) -> AppResult<String> {
    validate_ident(name)?;
    let client = pool.get().await?;
    let row = client
        .query_opt(
            "SELECT c.relkind::text, pg_get_viewdef(c.oid, true)
             FROM pg_class c
             JOIN pg_namespace n ON n.oid = c.relnamespace
             WHERE n.nspname = $1 AND c.relname = $2 AND c.relkind IN ('v', 'm')",
            &[&schema, &name],
        )
        .await?
        .ok_or_else(|| AppError::msg("view was not found"))?;
    let kind: String = row.get(0);
    let body: String = row.get(1);
    let keyword = if kind == "m" {
        "MATERIALIZED VIEW"
    } else {
        "VIEW"
    };
    Ok(format!(
        "CREATE {keyword} {} AS\n{body}",
        qualify(schema, name)
    ))
}

async fn function_ddl(pool: &Pool, schema: &str, name: &str) -> AppResult<String> {
    let client = pool.get().await?;
    let rows = client
        .query(
            "SELECT pg_get_functiondef(p.oid)
             FROM pg_proc p
             JOIN pg_namespace n ON n.oid = p.pronamespace
             WHERE n.nspname = $1
               AND (
                    p.proname = $2
                    OR p.proname || '(' || pg_get_function_identity_arguments(p.oid) || ')' = $2
               )
             ORDER BY p.oid",
            &[&schema, &name],
        )
        .await?;
    if rows.is_empty() {
        return Err(AppError::msg("routine was not found"));
    }
    Ok(rows
        .into_iter()
        .map(|row| row.get::<_, String>(0))
        .collect::<Vec<_>>()
        .join("\n\n"))
}

async fn trigger_ddl(pool: &Pool, schema: &str, name: &str) -> AppResult<String> {
    let client = pool.get().await?;
    let rows = client
        .query(
            "SELECT pg_get_triggerdef(t.oid, true)
             FROM pg_trigger t
             JOIN pg_class c ON c.oid = t.tgrelid
             JOIN pg_namespace n ON n.oid = c.relnamespace
             WHERE n.nspname = $1 AND t.tgname = $2 AND NOT t.tgisinternal
             ORDER BY t.oid",
            &[&schema, &name],
        )
        .await?;
    if rows.is_empty() {
        return Err(AppError::msg("trigger was not found"));
    }
    Ok(rows
        .into_iter()
        .map(|row| row.get::<_, String>(0))
        .collect::<Vec<_>>()
        .join(";\n\n"))
}

async fn index_ddl(pool: &Pool, schema: &str, name: &str) -> AppResult<String> {
    let client = pool.get().await?;
    let row = client
        .query_opt(
            "SELECT pg_get_indexdef(ic.oid)
             FROM pg_class ic
             JOIN pg_namespace n ON n.oid = ic.relnamespace
             JOIN pg_index i ON i.indexrelid = ic.oid
             WHERE n.nspname = $1 AND ic.relname = $2",
            &[&schema, &name],
        )
        .await?
        .ok_or_else(|| AppError::msg("index was not found"))?;
    let def: String = row.get(0);
    Ok(if def.ends_with(';') {
        def
    } else {
        format!("{def};")
    })
}
