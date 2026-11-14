use super::dump::{build_dump, build_table_data_dump, DumpDialect, DumpExtras, DumpStatement};
use super::engine::DatabaseEngine;
use super::ident::{
    create_pg_schema_sql, drop_pg_schema_sql, qualify_pg as qualify, quote_ident_pg as quote_ident,
    validate_ident,
};
use super::sql::{apply_default_query_limit, statement_kind, SqlDialect, DEFAULT_QUERY_ROW_LIMIT};
use crate::error::{AppError, AppResult};
use crate::models::{
    CharsetCatalog, ColumnInfo, ConnectionProfile, DatabaseInfo, ExplainPlan, IndexInfo,
    ObjectKind, QueryResult, RoutineInfo, SchemaObjectInfo, TableInfo, TestConnectionRequest,
    TriggerInfo, ViewInfo,
};
use async_trait::async_trait;
use deadpool_postgres::{Config, Manager, Pool, PoolConfig, Runtime, SslMode};
use native_tls::{Certificate, Identity, TlsConnector};
use notices::{NoticeConnect, NoticeRouter};
use postgres_native_tls::MakeTlsConnector;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tokio_postgres::{NoTls, SimpleQueryMessage};

mod notices;

const SYSTEM_SCHEMAS: [&str; 3] = ["pg_catalog", "information_schema", "pg_toast"];
const MAX_RESULT_ROWS: usize = 5_000;

#[derive(Clone)]
pub struct PostgresEngine {
    pool: Pool,
    notices: Arc<NoticeRouter>,
}

impl PostgresEngine {
    pub fn from_profile(profile: &ConnectionProfile) -> AppResult<Self> {
        let notices = Arc::new(NoticeRouter::default());
        Ok(Self {
            notices: notices.clone(),
            pool: create_pool(
                &profile.host,
                profile.port,
                &profile.username,
                profile.password.as_deref(),
                profile.database.as_deref(),
                SslFiles {
                    ca: profile.ssl_ca.as_deref(),
                    cert: profile.ssl_cert.as_deref(),
                    key: profile.ssl_key.as_deref(),
                },
                notices,
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
            SslFiles {
                ca: request.ssl_ca.as_deref(),
                cert: request.ssl_cert.as_deref(),
                key: request.ssl_key.as_deref(),
            },
            Arc::default(),
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
                        CASE c.relkind
                            WHEN 'p' THEN 'partitioned'
                            WHEN 'f' THEN 'foreign'
                            ELSE COALESCE(am.amname, 'heap')
                        END,
                        GREATEST(c.reltuples, 0)::bigint,
                        pg_total_relation_size(c.oid),
                        COALESCE(obj_description(c.oid, 'pg_class'), ''),
                        parent.relname
                 FROM pg_class c
                 JOIN pg_namespace n ON n.oid = c.relnamespace
                 LEFT JOIN pg_am am ON am.oid = c.relam
                 LEFT JOIN pg_inherits inh ON c.relispartition AND inh.inhrelid = c.oid
                 LEFT JOIN pg_class parent ON parent.oid = inh.inhparent
                 WHERE n.nspname = $1
                   AND c.relkind IN ('r', 'p', 'f')
                 ORDER BY c.relispartition, c.relkind <> 'p', c.relname",
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
                parent: row.get(5),
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
                   AND NOT EXISTS (
                        SELECT 1 FROM pg_depend d
                        WHERE d.classid = 'pg_proc'::regclass
                          AND d.objid = p.oid
                          AND d.deptype = 'e'
                   )
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
                            WHEN a.attgenerated = 'v' THEN 'GENERATED ALWAYS AS VIRTUAL'
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
            ObjectKind::Sequence
            | ObjectKind::Type
            | ObjectKind::Extension
            | ObjectKind::Policy => extra_object_ddl(&self.pool, schema, kind, name).await,
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

        let capture = self.notices.begin(&client).await?;
        let started = Instant::now();
        let result = client.simple_query(sql).await;
        let notices = capture.finish();
        if schema.is_some() {
            let _ = client.simple_query("RESET search_path").await;
        }
        let query_messages = match result {
            Ok(query_messages) => query_messages,
            Err(error) if !notices.is_empty() => {
                let mut message = AppError::from(error).user_message();
                for notice in &notices {
                    message.push('\n');
                    message.push_str(&notice.text);
                }
                return Err(AppError::msg(message));
            }
            Err(error) => return Err(error.into()),
        };
        messages.extend(notices);
        Ok(simple_query_result(
            query_messages,
            sql,
            started.elapsed().as_millis() as u64,
            messages,
            limited.applied,
        ))
    }

    async fn dump_extras(&self, schema: &str) -> AppResult<DumpExtras> {
        validate_ident(schema)?;
        dump_extras(&self.pool, schema).await
    }

    async fn list_server_databases(&self) -> AppResult<Vec<String>> {
        let client = self.pool.get().await?;
        let rows = client
            .query(
                "SELECT datname
                 FROM pg_database
                 WHERE datallowconn
                   AND NOT datistemplate
                   AND has_database_privilege(datname, 'CONNECT')
                 ORDER BY datname",
                &[],
            )
            .await?;
        Ok(rows.into_iter().map(|row| row.get(0)).collect())
    }

    async fn list_schema_objects(
        &self,
        schema: &str,
        kind: ObjectKind,
    ) -> AppResult<Vec<SchemaObjectInfo>> {
        validate_ident(schema)?;
        let sql = match kind {
            ObjectKind::Sequence => {
                "SELECT c.relname,
                        pg_catalog.format_type(s.seqtypid, NULL)
                        || COALESCE(' · ' || CASE dep.deptype WHEN 'i' THEN 'identity ' ELSE 'owned by ' END
                                    || tc.relname || '.' || a.attname, '')
                 FROM pg_sequence s
                 JOIN pg_class c ON c.oid = s.seqrelid
                 JOIN pg_namespace n ON n.oid = c.relnamespace
                 LEFT JOIN pg_depend dep
                   ON dep.classid = 'pg_class'::regclass AND dep.objid = c.oid
                  AND dep.refclassid = 'pg_class'::regclass AND dep.deptype IN ('a', 'i')
                 LEFT JOIN pg_class tc ON tc.oid = dep.refobjid
                 LEFT JOIN pg_attribute a ON a.attrelid = dep.refobjid AND a.attnum = dep.refobjsubid
                 WHERE n.nspname = $1
                   AND NOT EXISTS (SELECT 1 FROM pg_depend e
                                   WHERE e.classid = 'pg_class'::regclass AND e.objid = c.oid
                                     AND e.deptype = 'e')
                 ORDER BY c.relname"
            }
            ObjectKind::Type => {
                "SELECT t.typname,
                        CASE t.typtype
                            WHEN 'e' THEN 'enum (' || (SELECT count(*) FROM pg_enum e WHERE e.enumtypid = t.oid) || ')'
                            WHEN 'd' THEN 'domain · ' || pg_catalog.format_type(t.typbasetype, t.typtypmod)
                            WHEN 'r' THEN 'range · ' || (SELECT pg_catalog.format_type(r.rngsubtype, NULL)
                                                        FROM pg_range r WHERE r.rngtypid = t.oid)
                            ELSE 'composite'
                        END
                 FROM pg_type t
                 JOIN pg_namespace n ON n.oid = t.typnamespace
                 LEFT JOIN pg_class c ON c.oid = t.typrelid
                 WHERE n.nspname = $1
                   AND (t.typtype IN ('e', 'd', 'r') OR (t.typtype = 'c' AND c.relkind = 'c'))
                   AND NOT EXISTS (SELECT 1 FROM pg_depend e
                                   WHERE e.classid = 'pg_type'::regclass AND e.objid = t.oid
                                     AND e.deptype = 'e')
                 ORDER BY t.typname"
            }
            ObjectKind::Extension => {
                "SELECT e.extname, 'v' || e.extversion
                 FROM pg_extension e
                 JOIN pg_namespace n ON n.oid = e.extnamespace
                 WHERE n.nspname = $1
                 ORDER BY e.extname"
            }
            ObjectKind::Policy => {
                "SELECT p.tablename || '.' || p.policyname,
                        p.cmd || ' · ' || p.permissive || ' · ' || array_to_string(p.roles, ', ')
                 FROM pg_policies p
                 WHERE p.schemaname = $1
                 ORDER BY p.tablename, p.policyname"
            }
            _ => return Ok(Vec::new()),
        };
        let client = self.pool.get().await?;
        let rows = client.query(sql, &[&schema]).await?;
        Ok(rows
            .into_iter()
            .map(|row| SchemaObjectInfo {
                name: row.get(0),
                detail: row.get::<_, Option<String>>(1).unwrap_or_default(),
            })
            .collect())
    }

    async fn explain_plan(
        &self,
        schema: Option<&str>,
        sql: &str,
        analyze: bool,
    ) -> AppResult<ExplainPlan> {
        let target = sql
            .trim()
            .trim_end_matches(|c: char| c == ';' || c.is_whitespace());
        if target.is_empty() {
            return Err(AppError::msg("SQL is empty"));
        }
        if let Some(schema) = schema {
            validate_ident(schema)?;
        }
        let mut client = self.pool.get().await?;
        // ANALYZE executes the statement: keep it inside a transaction that is
        // always rolled back. The extended protocol rejects a second statement,
        // so nothing can COMMIT behind our back.
        let tx = client.transaction().await?;
        if let Some(schema) = schema {
            tx.batch_execute(&format!(
                "SET LOCAL search_path TO {}, pg_catalog",
                quote_ident(schema)
            ))
            .await?;
        }
        let options = if analyze {
            "ANALYZE, BUFFERS, VERBOSE, COSTS, FORMAT JSON"
        } else {
            "VERBOSE, COSTS, FORMAT JSON"
        };
        let started = Instant::now();
        let rows = tx
            .query(&format!("EXPLAIN ({options}) {target}"), &[])
            .await;
        let duration_ms = started.elapsed().as_millis() as u64;
        tx.rollback().await?;
        let plan = rows?
            .first()
            .map(|row| row.get::<_, JsonText>(0).0)
            .ok_or_else(|| AppError::msg("EXPLAIN returned no plan"))?;
        Ok(ExplainPlan {
            plan,
            analyzed: analyze,
            duration_ms,
        })
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
    ssl: SslFiles<'_>,
    notices: Arc<NoticeRouter>,
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
    let tls = tls_connector_from_files(ssl)?;
    if tls.is_some() {
        cfg.ssl_mode = Some(SslMode::Require);
    }
    let pg_config = cfg
        .get_pg_config()
        .map_err(|error| AppError::PgPool(error.to_string()))?;
    let manager = match tls {
        Some(tls) => Manager::from_connect(
            pg_config,
            NoticeConnect {
                tls,
                router: notices,
            },
            cfg.get_manager_config(),
        ),
        None => Manager::from_connect(
            pg_config,
            NoticeConnect {
                tls: NoTls,
                router: notices,
            },
            cfg.get_manager_config(),
        ),
    };
    Pool::builder(manager)
        .config(cfg.get_pool_config())
        .runtime(Runtime::Tokio1)
        .build()
        .map_err(|error| AppError::PgPool(error.to_string()))
}

/// `json` / `jsonb` column read as text, without the serde_json driver feature.
struct JsonText(String);

impl<'a> tokio_postgres::types::FromSql<'a> for JsonText {
    fn from_sql(
        ty: &tokio_postgres::types::Type,
        raw: &'a [u8],
    ) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        // Binary jsonb carries a one-byte version header before the text.
        let text = if *ty == tokio_postgres::types::Type::JSONB {
            raw.get(1..).unwrap_or_default()
        } else {
            raw
        };
        Ok(Self(std::str::from_utf8(text)?.to_string()))
    }

    fn accepts(ty: &tokio_postgres::types::Type) -> bool {
        use tokio_postgres::types::Type;
        matches!(*ty, Type::JSON | Type::JSONB | Type::TEXT)
    }
}

#[derive(Clone, Copy)]
struct SslFiles<'a> {
    ca: Option<&'a str>,
    cert: Option<&'a str>,
    key: Option<&'a str>,
}

fn trim_opt(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|item| !item.is_empty())
}

fn tls_connector_from_files(ssl: SslFiles<'_>) -> AppResult<Option<MakeTlsConnector>> {
    let ca = trim_opt(ssl.ca);
    let cert = trim_opt(ssl.cert);
    let key = trim_opt(ssl.key);
    if ca.is_none() && cert.is_none() && key.is_none() {
        return Ok(None);
    }
    if cert.is_some() != key.is_some() {
        return Err(AppError::msg(
            "SSL client certificate and key must be provided together",
        ));
    }

    let mut builder = TlsConnector::builder();
    if let Some(ca) = ca {
        let path = PathBuf::from(ca);
        if !path.is_file() {
            return Err(AppError::msg(format!("CA certificate not found: {ca}")));
        }
        for certificate in load_root_certs(&path)? {
            builder.add_root_certificate(certificate);
        }
        // Match libpq sslmode=verify-ca: trust the supplied CA without requiring hostname SANs.
        builder.danger_accept_invalid_hostnames(true);
    }
    if let (Some(cert), Some(key)) = (cert, key) {
        builder.identity(load_client_identity(cert, key)?);
    }

    let connector = builder
        .build()
        .map_err(|error| AppError::msg(format!("failed to build TLS connector: {error}")))?;
    Ok(Some(MakeTlsConnector::new(connector)))
}

fn load_root_certs(path: &Path) -> AppResult<Vec<Certificate>> {
    let bytes = std::fs::read(path)?;
    let text = String::from_utf8_lossy(&bytes);
    let mut certs = Vec::new();
    let mut rest = text.as_ref();
    const BEGIN: &str = "-----BEGIN CERTIFICATE-----";
    const END: &str = "-----END CERTIFICATE-----";
    while let Some(start) = rest.find(BEGIN) {
        let Some(end_rel) = rest[start..].find(END) else {
            return Err(AppError::msg("invalid CA certificate: truncated PEM"));
        };
        let end = start + end_rel + END.len();
        let pem = &rest.as_bytes()[start..end];
        certs.push(
            Certificate::from_pem(pem)
                .map_err(|error| AppError::msg(format!("invalid CA certificate: {error}")))?,
        );
        rest = &rest[end..];
    }
    if certs.is_empty() {
        certs.push(
            Certificate::from_der(&bytes)
                .map_err(|error| AppError::msg(format!("invalid CA certificate: {error}")))?,
        );
    }
    Ok(certs)
}

fn load_client_identity(cert: &str, key: &str) -> AppResult<Identity> {
    let cert_path = PathBuf::from(cert);
    let key_path = PathBuf::from(key);
    if !cert_path.is_file() {
        return Err(AppError::msg(format!(
            "client certificate not found: {cert}"
        )));
    }
    if !key_path.is_file() {
        return Err(AppError::msg(format!("client key not found: {key}")));
    }
    let cert_bytes = std::fs::read(&cert_path)?;
    let key_bytes = std::fs::read(&key_path)?;
    Identity::from_pkcs8(&cert_bytes, &key_bytes)
        .map_err(|error| AppError::msg(format!("invalid SSL client certificate or key: {error}")))
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

/// Catalog reads inside a transaction whose search_path is `schema`, so DDL
/// helpers print same-schema names unqualified and they resolve against
/// whichever schema the script is restored into.
async fn schema_scoped<'a>(
    client: &'a mut deadpool_postgres::Client,
    schema: &str,
) -> AppResult<deadpool_postgres::Transaction<'a>> {
    let tx = client.transaction().await?;
    tx.batch_execute(&format!(
        "SET LOCAL search_path TO {}, pg_catalog",
        quote_ident(schema)
    ))
    .await?;
    Ok(tx)
}

fn pg_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn column_definition(
    name: &str,
    type_name: &str,
    not_null: bool,
    default_expr: Option<&str>,
    identity: &str,
    generated: &str,
) -> String {
    let mut line = format!("    {} {}", quote_ident(name), type_name);
    match (identity, generated, default_expr) {
        ("a", _, _) => line.push_str(" GENERATED ALWAYS AS IDENTITY"),
        ("d", _, _) => line.push_str(" GENERATED BY DEFAULT AS IDENTITY"),
        (_, "s", Some(expr)) => line.push_str(&format!(" GENERATED ALWAYS AS ({expr}) STORED")),
        (_, "v", Some(expr)) => line.push_str(&format!(" GENERATED ALWAYS AS ({expr}) VIRTUAL")),
        (_, _, Some(expr)) => line.push_str(&format!(" DEFAULT {expr}")),
        _ => {}
    }
    if not_null && identity.is_empty() {
        line.push_str(" NOT NULL");
    }
    line
}

/// `pg_get_indexdef` always schema-qualifies the table with minimal quoting,
/// which schema renames cannot match; point it at the quoted table instead.
/// `ON ONLY` (partitioned tables) is dropped so the index cascades to
/// partitions restored afterwards.
fn requalify_index_def(def: &str, printed_table: &str, qualified: &str) -> String {
    for on in [" ON ONLY ", " ON "] {
        let needle = format!("{on}{printed_table} ");
        if def.contains(&needle) {
            return def.replacen(&needle, &format!(" ON {qualified} "), 1);
        }
    }
    def.to_string()
}

async fn table_ddl(pool: &Pool, schema: &str, name: &str) -> AppResult<String> {
    validate_ident(name)?;
    let mut client = pool.get().await?;
    let tx = schema_scoped(&mut client, schema).await?;
    let relation = tx
        .query_opt(
            "SELECT c.oid,
                    CASE WHEN c.relkind = 'p' THEN pg_get_partkeydef(c.oid) END,
                    CASE WHEN c.relispartition THEN pg_get_expr(c.relpartbound, c.oid) END,
                    pn.nspname,
                    pc.relname,
                    obj_description(c.oid, 'pg_class'),
                    quote_ident(n.nspname) || '.' || quote_ident(c.relname)
             FROM pg_class c
             JOIN pg_namespace n ON n.oid = c.relnamespace
             LEFT JOIN pg_inherits inh ON c.relispartition AND inh.inhrelid = c.oid
             LEFT JOIN pg_class pc ON pc.oid = inh.inhparent
             LEFT JOIN pg_namespace pn ON pn.oid = pc.relnamespace
             WHERE n.nspname = $1
               AND c.relname = $2
               AND c.relkind IN ('r', 'p', 'f')",
            &[&schema, &name],
        )
        .await?
        .ok_or_else(|| AppError::msg("table was not found"))?;
    let oid: u32 = relation.get(0);
    let partition_key: Option<String> = relation.get(1);
    let partition_bound: Option<String> = relation.get(2);
    let parent = match (
        relation.get::<_, Option<String>>(3),
        relation.get::<_, Option<String>>(4),
    ) {
        (Some(parent_schema), Some(parent_name)) => Some(qualify(&parent_schema, &parent_name)),
        _ => None,
    };
    let table_comment: Option<String> = relation.get(5);
    let printed_table: String = relation.get(6);
    let is_partition = parent.is_some() && partition_bound.is_some();

    let columns = tx
        .query(
            "SELECT a.attname,
                    pg_catalog.format_type(a.atttypid, a.atttypmod),
                    a.attnotnull,
                    pg_get_expr(ad.adbin, ad.adrelid),
                    a.attidentity::text,
                    a.attgenerated::text,
                    col_description(a.attrelid, a.attnum)
             FROM pg_attribute a
             LEFT JOIN pg_attrdef ad
               ON ad.adrelid = a.attrelid AND ad.adnum = a.attnum
             WHERE a.attrelid = $1
               AND a.attnum > 0
               AND NOT a.attisdropped
             ORDER BY a.attnum",
            &[&oid],
        )
        .await?;

    let mut lines = Vec::new();
    let mut column_comments = Vec::new();
    for row in &columns {
        let col_name: String = row.get(0);
        let type_name: String = row.get(1);
        let not_null: bool = row.get(2);
        let default_expr: Option<String> = row.get(3);
        let identity: String = row.get(4);
        let generated: String = row.get(5);
        let comment: Option<String> = row.get(6);
        if !is_partition {
            lines.push(column_definition(
                &col_name,
                &type_name,
                not_null,
                default_expr.as_deref(),
                identity.trim_matches('\0'),
                generated.trim_matches('\0'),
            ));
        }
        if let Some(comment) = comment.filter(|value| !value.is_empty()) {
            column_comments.push((col_name, comment));
        }
    }

    // PostgreSQL 18 also stores NOT NULL in pg_constraint (contype 'n'); the
    // column lines above already carry it.
    let constraints = tx
        .query(
            "SELECT pg_get_constraintdef(co.oid, true)
             FROM pg_constraint co
             WHERE co.conrelid = $1
               AND co.conparentid = 0
               AND co.contype <> 'n'
               AND (NOT $2 OR co.conislocal)
             ORDER BY CASE co.contype
                        WHEN 'p' THEN 0
                        WHEN 'u' THEN 1
                        WHEN 'c' THEN 2
                        WHEN 'f' THEN 3
                        ELSE 4
                      END,
                      co.conname",
            &[&oid, &is_partition],
        )
        .await?;
    for row in constraints {
        let def: String = row.get(0);
        lines.push(format!("    {def}"));
    }

    let qualified = qualify(schema, name);
    let mut ddl = match (&parent, &partition_bound) {
        (Some(parent), Some(bound)) if is_partition => {
            let mut sql = format!("CREATE TABLE {qualified} PARTITION OF {parent}");
            if !lines.is_empty() {
                sql.push_str(&format!(" (\n{}\n)", lines.join(",\n")));
            }
            sql.push_str(&format!("\n{bound}"));
            sql
        }
        _ => format!("CREATE TABLE {qualified} (\n{}\n)", lines.join(",\n")),
    };
    if let Some(key) = &partition_key {
        ddl.push_str(&format!("\nPARTITION BY {key}"));
    }
    ddl.push(';');

    let indexes = tx
        .query(
            "SELECT pg_get_indexdef(i.indexrelid)
             FROM pg_index i
             WHERE i.indrelid = $1
               AND NOT i.indisprimary
               AND NOT EXISTS (
                    SELECT 1 FROM pg_constraint co
                    WHERE co.conrelid = i.indrelid
                      AND co.conindid = i.indexrelid
                      AND co.contype IN ('p', 'u', 'x')
               )
               AND NOT EXISTS (
                    SELECT 1 FROM pg_inherits inh WHERE inh.inhrelid = i.indexrelid
               )
             ORDER BY i.indexrelid",
            &[&oid],
        )
        .await?;
    for row in indexes {
        let def = requalify_index_def(&row.get::<_, String>(0), &printed_table, &qualified);
        ddl.push_str("\n\n");
        ddl.push_str(&def);
        if !def.ends_with(';') {
            ddl.push(';');
        }
    }

    if let Some(comment) = table_comment.filter(|value| !value.is_empty()) {
        ddl.push_str(&format!(
            "\n\nCOMMENT ON TABLE {qualified} IS {};",
            pg_literal(&comment)
        ));
    }
    for (column, comment) in column_comments {
        ddl.push_str(&format!(
            "\nCOMMENT ON COLUMN {qualified}.{} IS {};",
            quote_ident(&column),
            pg_literal(&comment)
        ));
    }

    tx.commit().await?;
    Ok(ddl)
}

async fn view_ddl(pool: &Pool, schema: &str, name: &str) -> AppResult<String> {
    validate_ident(name)?;
    let mut client = pool.get().await?;
    let tx = schema_scoped(&mut client, schema).await?;
    let row = tx
        .query_opt(
            "SELECT c.relkind::text, pg_get_viewdef(c.oid, true), obj_description(c.oid, 'pg_class')
             FROM pg_class c
             JOIN pg_namespace n ON n.oid = c.relnamespace
             WHERE n.nspname = $1 AND c.relname = $2 AND c.relkind IN ('v', 'm')",
            &[&schema, &name],
        )
        .await?
        .ok_or_else(|| AppError::msg("view was not found"))?;
    tx.commit().await?;
    let kind: String = row.get(0);
    let body: String = row.get(1);
    let comment: Option<String> = row.get(2);
    let keyword = if kind == "m" {
        "MATERIALIZED VIEW"
    } else {
        "VIEW"
    };
    let qualified = qualify(schema, name);
    let mut ddl = format!("CREATE {keyword} {qualified} AS\n{body}");
    if let Some(comment) = comment.filter(|value| !value.is_empty()) {
        if !ddl.trim_end().ends_with(';') {
            ddl.push(';');
        }
        ddl.push_str(&format!(
            "\n\nCOMMENT ON {keyword} {qualified} IS {};",
            pg_literal(&comment)
        ));
    }
    Ok(ddl)
}

const NOT_EXTENSION_MEMBER: &str = "NOT EXISTS (
    SELECT 1 FROM pg_depend d
    WHERE d.classid = $2::text::regclass AND d.objid = t.oid AND d.deptype = 'e'
)";

struct SequenceSpec {
    name: String,
    data_type: String,
    start: i64,
    increment: i64,
    min: i64,
    max: i64,
    cache: i64,
    cycle: bool,
    /// `a` for serial-style ownership, `i` for identity columns.
    owner_kind: Option<String>,
    owner_table: Option<(String, String)>,
    owner_column: Option<String>,
    last_value: Option<i64>,
}

fn dump_statement(kind: &'static str, name: &str, label: String, sql: String) -> DumpStatement {
    DumpStatement {
        object_kind: Some(kind),
        object_name: Some(name.to_string()),
        label,
        sql,
    }
}

fn create_sequence_sql(schema: &str, seq: &SequenceSpec) -> String {
    format!(
        "CREATE SEQUENCE {} AS {} INCREMENT BY {} MINVALUE {} MAXVALUE {} START WITH {} CACHE {} {}",
        qualify(schema, &seq.name),
        seq.data_type,
        seq.increment,
        seq.min,
        seq.max,
        seq.start,
        seq.cache,
        if seq.cycle { "CYCLE" } else { "NO CYCLE" }
    )
}

/// Identity sequences are recreated by the column itself, so their position is
/// restored through `pg_get_serial_sequence` instead of the source name.
fn setval_sql(schema: &str, seq: &SequenceSpec) -> Option<String> {
    let value = seq.last_value?;
    let target = match (
        seq.owner_kind.as_deref(),
        &seq.owner_table,
        &seq.owner_column,
    ) {
        (Some("i"), Some((table_schema, table)), Some(column)) => format!(
            "pg_catalog.pg_get_serial_sequence({}, {})",
            pg_literal(&qualify(table_schema, table)),
            pg_literal(column)
        ),
        _ => pg_literal(&qualify(schema, &seq.name)),
    };
    Some(format!("SELECT pg_catalog.setval({target}, {value}, true)"))
}

async fn dump_extras(pool: &Pool, schema: &str) -> AppResult<DumpExtras> {
    let mut client = pool.get().await?;
    let tx = schema_scoped(&mut client, schema).await?;
    let mut extras = DumpExtras::default();
    let schema_q = quote_ident(schema);

    for row in tx
        .query(
            "SELECT e.extname
             FROM pg_extension e
             JOIN pg_namespace n ON n.oid = e.extnamespace
             WHERE n.nspname = $1
             ORDER BY e.oid",
            &[&schema],
        )
        .await?
    {
        let name: String = row.get(0);
        extras.prelude.push(dump_statement(
            "extension",
            &name,
            format!("Create extension {name}"),
            format!(
                "CREATE EXTENSION IF NOT EXISTS {} WITH SCHEMA {schema_q}",
                quote_ident(&name)
            ),
        ));
    }

    for row in tx
        .query(
            "SELECT c.relname, c.relkind::text, EXISTS (
                    SELECT 1 FROM pg_depend d
                    WHERE d.classid = 'pg_class'::regclass AND d.objid = c.oid AND d.deptype = 'e'
                )
             FROM pg_class c
             JOIN pg_namespace n ON n.oid = c.relnamespace
             WHERE n.nspname = $1
               AND c.relkind IN ('r', 'p', 'f', 'v', 'm')",
            &[&schema],
        )
        .await?
    {
        let name: String = row.get(0);
        let relkind: String = row.get(1);
        let extension_member: bool = row.get(2);
        if extension_member {
            extras.extension_relations.insert(name.clone());
        }
        if relkind == "p" {
            extras.partitioned_tables.insert(name);
        }
    }

    let enum_sql = format!(
        "SELECT t.typname, array_agg(e.enumlabel::text ORDER BY e.enumsortorder)
         FROM pg_type t
         JOIN pg_namespace n ON n.oid = t.typnamespace
         JOIN pg_enum e ON e.enumtypid = t.oid
         WHERE n.nspname = $1 AND {NOT_EXTENSION_MEMBER}
         GROUP BY t.oid, t.typname
         ORDER BY t.oid"
    );
    for row in tx.query(&enum_sql, &[&schema, &"pg_type"]).await? {
        let name: String = row.get(0);
        let labels: Vec<String> = row.get(1);
        let labels = labels
            .iter()
            .map(|label| pg_literal(label))
            .collect::<Vec<_>>()
            .join(", ");
        extras.prelude.push(dump_statement(
            "type",
            &name,
            format!("Create enum type {name}"),
            format!("CREATE TYPE {} AS ENUM ({labels})", qualify(schema, &name)),
        ));
    }

    let domain_sql = format!(
        "SELECT t.typname,
                pg_catalog.format_type(t.typbasetype, t.typtypmod),
                t.typnotnull,
                t.typdefault,
                COALESCE((
                    SELECT string_agg(' CONSTRAINT ' || quote_ident(co.conname) || ' '
                                      || pg_get_constraintdef(co.oid, true), '' ORDER BY co.conname)
                    FROM pg_constraint co
                    WHERE co.contypid = t.oid AND co.contype = 'c'
                ), '')
         FROM pg_type t
         JOIN pg_namespace n ON n.oid = t.typnamespace
         WHERE n.nspname = $1 AND t.typtype = 'd' AND {NOT_EXTENSION_MEMBER}
         ORDER BY t.oid"
    );
    for row in tx.query(&domain_sql, &[&schema, &"pg_type"]).await? {
        let name: String = row.get(0);
        let base: String = row.get(1);
        let not_null: bool = row.get(2);
        let default_expr: Option<String> = row.get(3);
        let checks: String = row.get(4);
        let mut sql = format!("CREATE DOMAIN {} AS {base}", qualify(schema, &name));
        if let Some(default_expr) = default_expr {
            sql.push_str(&format!(" DEFAULT {default_expr}"));
        }
        if not_null {
            sql.push_str(" NOT NULL");
        }
        sql.push_str(&checks);
        extras.prelude.push(dump_statement(
            "type",
            &name,
            format!("Create domain {name}"),
            sql,
        ));
    }

    let range_sql = format!(
        "SELECT t.typname,
                pg_catalog.format_type(r.rngsubtype, NULL),
                CASE WHEN r.rngsubdiff <> 0 THEN r.rngsubdiff::regproc::text END,
                CASE WHEN r.rngcollation NOT IN (0, 100)
                     THEN (SELECT quote_ident(co.collname) FROM pg_collation co
                           WHERE co.oid = r.rngcollation) END
         FROM pg_type t
         JOIN pg_namespace n ON n.oid = t.typnamespace
         JOIN pg_range r ON r.rngtypid = t.oid
         WHERE n.nspname = $1 AND {NOT_EXTENSION_MEMBER}
         ORDER BY t.oid"
    );
    for row in tx.query(&range_sql, &[&schema, &"pg_type"]).await? {
        let name: String = row.get(0);
        let subtype: String = row.get(1);
        let mut options = vec![format!("SUBTYPE = {subtype}")];
        if let Some(collation) = row.get::<_, Option<String>>(3) {
            options.push(format!("COLLATION = {collation}"));
        }
        if let Some(diff) = row.get::<_, Option<String>>(2) {
            options.push(format!("SUBTYPE_DIFF = {diff}"));
        }
        extras.prelude.push(dump_statement(
            "type",
            &name,
            format!("Create range type {name}"),
            format!(
                "CREATE TYPE {} AS RANGE ({})",
                qualify(schema, &name),
                options.join(", ")
            ),
        ));
    }

    let composite_sql = format!(
        "SELECT t.typname,
                string_agg(quote_ident(a.attname) || ' '
                           || pg_catalog.format_type(a.atttypid, a.atttypmod), ', ' ORDER BY a.attnum)
         FROM pg_type t
         JOIN pg_namespace n ON n.oid = t.typnamespace
         JOIN pg_class c ON c.oid = t.typrelid AND c.relkind = 'c'
         JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum > 0 AND NOT a.attisdropped
         WHERE n.nspname = $1 AND t.typtype = 'c' AND {NOT_EXTENSION_MEMBER}
         GROUP BY t.oid, t.typname
         ORDER BY t.oid"
    );
    for row in tx.query(&composite_sql, &[&schema, &"pg_type"]).await? {
        let name: String = row.get(0);
        let fields: String = row.get(1);
        extras.prelude.push(dump_statement(
            "type",
            &name,
            format!("Create composite type {name}"),
            format!("CREATE TYPE {} AS ({fields})", qualify(schema, &name)),
        ));
    }

    let sequence_sql = format!(
        "SELECT t.relname,
                pg_catalog.format_type(s.seqtypid, NULL),
                s.seqstart, s.seqincrement, s.seqmin, s.seqmax, s.seqcache, s.seqcycle,
                dep.deptype::text,
                tn.nspname,
                tc.relname,
                a.attname,
                CASE WHEN has_sequence_privilege(t.oid, 'SELECT,USAGE')
                     THEN pg_sequence_last_value(t.oid) END
         FROM pg_sequence s
         JOIN pg_class t ON t.oid = s.seqrelid
         JOIN pg_namespace n ON n.oid = t.relnamespace
         LEFT JOIN pg_depend dep
           ON dep.classid = 'pg_class'::regclass
          AND dep.objid = t.oid
          AND dep.refclassid = 'pg_class'::regclass
          AND dep.deptype IN ('a', 'i')
         LEFT JOIN pg_class tc ON tc.oid = dep.refobjid
         LEFT JOIN pg_namespace tn ON tn.oid = tc.relnamespace
         LEFT JOIN pg_attribute a ON a.attrelid = dep.refobjid AND a.attnum = dep.refobjsubid
         WHERE n.nspname = $1 AND {NOT_EXTENSION_MEMBER}
         ORDER BY t.relname"
    );
    for row in tx.query(&sequence_sql, &[&schema, &"pg_class"]).await? {
        let owner_table = match (
            row.get::<_, Option<String>>(9),
            row.get::<_, Option<String>>(10),
        ) {
            (Some(table_schema), Some(table)) => Some((table_schema, table)),
            _ => None,
        };
        let seq = SequenceSpec {
            name: row.get(0),
            data_type: row.get(1),
            start: row.get(2),
            increment: row.get(3),
            min: row.get(4),
            max: row.get(5),
            cache: row.get(6),
            cycle: row.get(7),
            owner_kind: row.get(8),
            owner_table,
            owner_column: row.get(11),
            last_value: row.get(12),
        };
        let identity = seq.owner_kind.as_deref() == Some("i");
        if !identity {
            extras.prelude.push(dump_statement(
                "sequence",
                &seq.name,
                format!("Create sequence {}", seq.name),
                create_sequence_sql(schema, &seq),
            ));
            if let (Some((table_schema, table)), Some(column)) =
                (&seq.owner_table, &seq.owner_column)
            {
                extras.postlude.push(dump_statement(
                    "sequence",
                    &seq.name,
                    format!("Set owner of sequence {}", seq.name),
                    format!(
                        "ALTER SEQUENCE {} OWNED BY {}.{}",
                        qualify(schema, &seq.name),
                        qualify(table_schema, table),
                        quote_ident(column)
                    ),
                ));
            }
        }
        if let Some(sql) = setval_sql(schema, &seq) {
            extras.sequence_values.push(dump_statement(
                "sequence",
                &seq.name,
                format!("Restore value of sequence {}", seq.name),
                sql,
            ));
        }
    }

    for row in tx
        .query(
            "SELECT c.relname, c.relforcerowsecurity
             FROM pg_class c
             JOIN pg_namespace n ON n.oid = c.relnamespace
             WHERE n.nspname = $1 AND c.relrowsecurity AND c.relkind IN ('r', 'p')
             ORDER BY c.relname",
            &[&schema],
        )
        .await?
    {
        let table: String = row.get(0);
        let force: bool = row.get(1);
        let qualified = qualify(schema, &table);
        extras.finale.push(dump_statement(
            "row_security",
            &table,
            format!("Enable row level security on {table}"),
            format!("ALTER TABLE {qualified} ENABLE ROW LEVEL SECURITY"),
        ));
        if force {
            extras.finale.push(dump_statement(
                "row_security",
                &table,
                format!("Force row level security on {table}"),
                format!("ALTER TABLE {qualified} FORCE ROW LEVEL SECURITY"),
            ));
        }
    }

    for row in tx
        .query(
            "SELECT tablename::text, policyname::text, permissive, roles::text[], cmd, qual, with_check
             FROM pg_policies
             WHERE schemaname = $1
             ORDER BY tablename, policyname",
            &[&schema],
        )
        .await?
    {
        let table: String = row.get(0);
        let policy: String = row.get(1);
        let permissive: String = row.get(2);
        let roles: Vec<String> = row.get(3);
        let cmd: String = row.get(4);
        let qual: Option<String> = row.get(5);
        let with_check: Option<String> = row.get(6);
        let roles = roles
            .iter()
            .map(|role| {
                if role == "public" {
                    "PUBLIC".to_string()
                } else {
                    quote_ident(role)
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        let mut sql = format!(
            "CREATE POLICY {} ON {} AS {permissive} FOR {cmd} TO {roles}",
            quote_ident(&policy),
            qualify(schema, &table)
        );
        if let Some(qual) = qual {
            sql.push_str(&format!(" USING ({qual})"));
        }
        if let Some(with_check) = with_check {
            sql.push_str(&format!(" WITH CHECK ({with_check})"));
        }
        extras.finale.push(dump_statement(
            "policy",
            &format!("{table}.{policy}"),
            format!("Create policy {policy} on {table}"),
            sql,
        ));
    }

    tx.commit().await?;
    Ok(extras)
}

/// DDL of a sequence, type, extension or policy, assembled from the same
/// statements a schema dump would emit for it.
async fn extra_object_ddl(
    pool: &Pool,
    schema: &str,
    kind: ObjectKind,
    name: &str,
) -> AppResult<String> {
    let extras = dump_extras(pool, schema).await?;
    let label = match kind {
        ObjectKind::Sequence => "sequence",
        ObjectKind::Type => "type",
        ObjectKind::Extension => "extension",
        _ => "policy",
    };
    let table = name.split_once('.').map(|(table, _)| table);
    let statements: Vec<&DumpStatement> = extras
        .prelude
        .iter()
        .chain(&extras.postlude)
        .chain(&extras.finale)
        .filter(|statement| match statement.object_kind {
            Some(found) if found == label => statement.object_name.as_deref() == Some(name),
            Some("row_security") if kind == ObjectKind::Policy => {
                statement.object_name.as_deref() == table
            }
            _ => false,
        })
        .collect();
    let current_value = (kind == ObjectKind::Sequence)
        .then(|| {
            extras
                .sequence_values
                .iter()
                .find(|statement| statement.object_name.as_deref() == Some(name))
        })
        .flatten();
    if !statements
        .iter()
        .any(|statement| statement.object_kind == Some(label))
    {
        if kind == ObjectKind::Sequence {
            if let Some(column) = identity_column(pool, schema, name).await? {
                let mut ddl = format!(
                    "-- {name} belongs to the identity column {column} and is created by its \
                     GENERATED ... AS IDENTITY clause."
                );
                if let Some(value) = current_value {
                    ddl.push_str(&format!("\n{};", value.sql));
                }
                return Ok(ddl);
            }
        }
        return Err(AppError::msg(format!("{label} was not found")));
    }
    let mut ddl = statements
        .iter()
        .map(|statement| format!("{};", statement.sql))
        .collect::<Vec<_>>()
        .join("\n\n");
    if let Some(value) = current_value {
        ddl.push_str(&format!("\n\n-- current value\n{};", value.sql));
    }
    Ok(ddl)
}

async fn identity_column(pool: &Pool, schema: &str, sequence: &str) -> AppResult<Option<String>> {
    let client = pool.get().await?;
    let row = client
        .query_opt(
            "SELECT tc.relname || '.' || a.attname
             FROM pg_class c
             JOIN pg_namespace n ON n.oid = c.relnamespace
             JOIN pg_depend d ON d.classid = 'pg_class'::regclass AND d.objid = c.oid
                             AND d.refclassid = 'pg_class'::regclass AND d.deptype = 'i'
             JOIN pg_class tc ON tc.oid = d.refobjid
             JOIN pg_attribute a ON a.attrelid = d.refobjid AND a.attnum = d.refobjsubid
             WHERE n.nspname = $1 AND c.relname = $2 AND c.relkind = 'S'",
            &[&schema, &sequence],
        )
        .await?;
    Ok(row.map(|row| row.get(0)))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn expect_tls_error(ssl: SslFiles<'_>) -> String {
        match tls_connector_from_files(ssl) {
            Ok(_) => panic!("expected TLS configuration error"),
            Err(error) => error.to_string(),
        }
    }

    #[test]
    fn renders_generated_columns_by_kind() {
        assert_eq!(
            column_definition("total", "numeric", false, Some("(qty * price)"), "", "s"),
            "    \"total\" numeric GENERATED ALWAYS AS ((qty * price)) STORED"
        );
        assert_eq!(
            column_definition("total", "numeric", false, Some("(qty * price)"), "", "v"),
            "    \"total\" numeric GENERATED ALWAYS AS ((qty * price)) VIRTUAL"
        );
        assert_eq!(
            column_definition("qty", "integer", true, Some("1"), "", ""),
            "    \"qty\" integer DEFAULT 1 NOT NULL"
        );
        assert_eq!(
            column_definition("id", "bigint", true, None, "a", ""),
            "    \"id\" bigint GENERATED ALWAYS AS IDENTITY"
        );
    }

    #[test]
    fn requalifies_index_target_table() {
        assert_eq!(
            requalify_index_def(
                "CREATE INDEX m_ts_idx ON ONLY shop.m USING btree (ts)",
                "shop.m",
                "\"shop\".\"m\""
            ),
            "CREATE INDEX m_ts_idx ON \"shop\".\"m\" USING btree (ts)"
        );
        assert_eq!(
            requalify_index_def(
                "CREATE UNIQUE INDEX \"Idx\" ON \"Shop\".orders USING btree (code)",
                "\"Shop\".orders",
                "\"Shop\".\"orders\""
            ),
            "CREATE UNIQUE INDEX \"Idx\" ON \"Shop\".\"orders\" USING btree (code)"
        );
    }

    fn sequence(owner_kind: Option<&str>, last_value: Option<i64>) -> SequenceSpec {
        SequenceSpec {
            name: "orders_id_seq".into(),
            data_type: "bigint".into(),
            start: 1,
            increment: 1,
            min: 1,
            max: i64::MAX,
            cache: 1,
            cycle: false,
            owner_kind: owner_kind.map(str::to_string),
            owner_table: owner_kind.map(|_| ("shop".to_string(), "orders".to_string())),
            owner_column: owner_kind.map(|_| "id".to_string()),
            last_value,
        }
    }

    #[test]
    fn builds_sequence_statements() {
        assert_eq!(
            create_sequence_sql("shop", &sequence(Some("a"), None)),
            "CREATE SEQUENCE \"shop\".\"orders_id_seq\" AS bigint INCREMENT BY 1 MINVALUE 1 \
             MAXVALUE 9223372036854775807 START WITH 1 CACHE 1 NO CYCLE"
        );
        assert_eq!(setval_sql("shop", &sequence(Some("a"), None)), None);
        assert_eq!(
            setval_sql("shop", &sequence(Some("a"), Some(42))).unwrap(),
            "SELECT pg_catalog.setval('\"shop\".\"orders_id_seq\"', 42, true)"
        );
        assert_eq!(
            setval_sql("shop", &sequence(Some("i"), Some(42))).unwrap(),
            "SELECT pg_catalog.setval(pg_catalog.pg_get_serial_sequence('\"shop\".\"orders\"', 'id'), 42, true)"
        );
    }

    #[test]
    fn escapes_comment_literals() {
        assert_eq!(pg_literal("客户's 订单"), "'客户''s 订单'");
    }

    #[test]
    fn skips_tls_when_certs_missing() {
        assert!(tls_connector_from_files(SslFiles {
            ca: None,
            cert: None,
            key: None
        })
        .unwrap()
        .is_none());
        assert!(tls_connector_from_files(SslFiles {
            ca: Some(""),
            cert: Some("  "),
            key: Some("")
        })
        .unwrap()
        .is_none());
    }

    #[test]
    fn rejects_missing_ca_file() {
        let error = expect_tls_error(SslFiles {
            ca: Some("/definitely/missing/ca.pem"),
            cert: None,
            key: None,
        });
        assert!(error.contains("CA certificate not found"));
    }

    #[test]
    fn rejects_client_cert_without_key() {
        let error = expect_tls_error(SslFiles {
            ca: None,
            cert: Some("/tmp/client.crt"),
            key: None,
        });
        assert!(error.contains("must be provided together"));
    }

    #[test]
    fn rejects_client_key_without_cert() {
        let error = expect_tls_error(SslFiles {
            ca: None,
            cert: None,
            key: Some("/tmp/client.key"),
        });
        assert!(error.contains("must be provided together"));
    }

    #[test]
    fn rejects_invalid_ca_file() {
        let path = std::env::temp_dir().join("db-gui-invalid-ca.pem");
        std::fs::write(&path, "not a certificate").unwrap();
        let error = expect_tls_error(SslFiles {
            ca: path.to_str(),
            cert: None,
            key: None,
        });
        assert!(error.contains("invalid CA certificate"));
    }
}
