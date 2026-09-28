use super::engine::DatabaseEngine;
use super::ident::{qualify_mssql, quote_ident, quote_ident_mssql, quote_ident_pg, validate_ident};
use crate::error::{AppError, AppResult};
use crate::models::ObjectKind;

#[derive(Clone, Copy)]
pub enum DumpDialect {
    Mysql,
    Postgres,
    Mssql,
    Oracle,
}

pub fn dump_mode_label(include_schema: bool, include_data: bool) -> &'static str {
    match (include_schema, include_data) {
        (true, true) => "schema + data",
        (true, false) => "schema only",
        (false, true) => "data only",
        (false, false) => "empty",
    }
}

#[derive(Debug, Clone)]
pub struct DumpStatement {
    pub object_kind: Option<&'static str>,
    pub object_name: Option<String>,
    pub label: String,
    pub sql: String,
}

#[derive(Debug, Clone, Default)]
pub struct DumpScript {
    pub statements: Vec<DumpStatement>,
}

impl DumpScript {
    pub fn full_sql(&self) -> String {
        let mut out = String::from("-- DB GUI dump\n");
        for statement in &self.statements {
            if let (Some(kind), Some(name)) =
                (statement.object_kind, statement.object_name.as_deref())
            {
                out.push_str(&format!("-- {kind}: {name}\n"));
            } else if !statement.label.is_empty() {
                out.push_str(&format!("-- {}\n", statement.label));
            }
            out.push_str(statement.sql.trim_end());
            if !statement.sql.trim_end().ends_with(';') {
                out.push(';');
            }
            out.push_str("\n\n");
        }
        out.push_str("-- Dump completed\n");
        out
    }
}

pub async fn build_dump(
    engine: &impl DatabaseEngine,
    schema: &str,
    include_schema: bool,
    include_data: bool,
    dialect: DumpDialect,
) -> AppResult<String> {
    let noop = |_phase: &str,
                _object_kind: Option<&str>,
                _object_name: Option<&str>,
                _current: u32,
                _total: u32,
                _message: &str| {};
    Ok(build_dump_script(
        engine,
        schema,
        include_schema,
        include_data,
        dialect,
        Some(noop),
    )
    .await?
    .full_sql())
}

pub async fn build_table_data_dump(
    engine: &impl DatabaseEngine,
    schema: &str,
    table: &str,
    dialect: DumpDialect,
) -> AppResult<String> {
    validate_ident(schema)?;
    validate_ident(table)?;

    let mut script = DumpScript::default();
    push_database_preamble(&mut script, schema, false, dialect);
    let noop = |_phase: &str,
                _object_kind: Option<&str>,
                _object_name: Option<&str>,
                _current: u32,
                _total: u32,
                _message: &str| {};
    let mut on_progress = Some(noop);
    append_table_data_statements(
        &mut script,
        engine,
        schema,
        table,
        dialect,
        &mut on_progress,
        1,
        1,
    )
    .await?;
    Ok(script.full_sql())
}

pub async fn build_dump_script<F>(
    engine: &impl DatabaseEngine,
    schema: &str,
    include_schema: bool,
    include_data: bool,
    dialect: DumpDialect,
    mut on_progress: Option<F>,
) -> AppResult<DumpScript>
where
    F: FnMut(&str, Option<&str>, Option<&str>, u32, u32, &str),
{
    validate_ident(schema)?;
    if !include_schema && !include_data {
        return Err(AppError::msg("dump must include schema or data"));
    }

    let mut script = DumpScript::default();
    let mode = dump_mode_label(include_schema, include_data);

    notify(
        &mut on_progress,
        "dump",
        None,
        None,
        0,
        0,
        &format!("Building dump for {schema} ({mode})"),
    );

    push_database_preamble(&mut script, schema, include_schema, dialect);

    let tables = engine.list_tables(schema).await?;
    let views = if include_schema {
        engine.list_views(schema).await?
    } else {
        Vec::new()
    };
    let routines = if include_schema {
        engine.list_routines(schema).await?
    } else {
        Vec::new()
    };
    let triggers = if include_schema {
        engine.list_triggers(schema).await?
    } else {
        Vec::new()
    };
    let total_objects =
        (tables.len() + views.len() + routines.len() + triggers.len()).max(1) as u32;
    let mut index = 0_u32;

    for table in &tables {
        index += 1;
        if include_schema {
            notify(
                &mut on_progress,
                "dump",
                Some("table"),
                Some(&table.name),
                index,
                total_objects,
                &format!("Dumping table DDL: {}", table.name),
            );
            match engine.get_ddl(schema, ObjectKind::Table, &table.name).await {
                Ok(ddl) => {
                    script.statements.push(DumpStatement {
                        object_kind: Some("table"),
                        object_name: Some(table.name.clone()),
                        label: format!("Create table {}", table.name),
                        sql: ddl.trim_end().trim_end_matches(';').to_string(),
                    });
                }
                Err(error) => {
                    notify(
                        &mut on_progress,
                        "dump",
                        Some("table"),
                        Some(&table.name),
                        index,
                        total_objects,
                        &format!("Skipped table DDL {}: {error}", table.name),
                    );
                }
            }
        }

        if include_data {
            append_table_data_statements(
                &mut script,
                engine,
                schema,
                &table.name,
                dialect,
                &mut on_progress,
                index,
                total_objects,
            )
            .await?;
        }
    }

    for view in &views {
        index += 1;
        notify(
            &mut on_progress,
            "dump",
            Some("view"),
            Some(&view.name),
            index,
            total_objects,
            &format!("Dumping view: {}", view.name),
        );
        match engine.get_ddl(schema, ObjectKind::View, &view.name).await {
            Ok(ddl) => {
                script.statements.push(DumpStatement {
                    object_kind: Some("view"),
                    object_name: Some(view.name.clone()),
                    label: format!("Create view {}", view.name),
                    sql: ddl.trim_end().trim_end_matches(';').to_string(),
                });
            }
            Err(error) => {
                notify(
                    &mut on_progress,
                    "dump",
                    Some("view"),
                    Some(&view.name),
                    index,
                    total_objects,
                    &format!("Skipped view {}: {error}", view.name),
                );
            }
        }
    }

    for routine in &routines {
        index += 1;
        let kind = if routine.routine_type.eq_ignore_ascii_case("PROCEDURE") {
            ObjectKind::Procedure
        } else {
            ObjectKind::Function
        };
        let kind_label = if matches!(kind, ObjectKind::Procedure) {
            "procedure"
        } else {
            "function"
        };
        notify(
            &mut on_progress,
            "dump",
            Some(kind_label),
            Some(&routine.name),
            index,
            total_objects,
            &format!("Dumping {kind_label}: {}", routine.name),
        );
        match engine.get_ddl(schema, kind, &routine.name).await {
            Ok(ddl) => {
                script.statements.push(DumpStatement {
                    object_kind: Some(kind_label),
                    object_name: Some(routine.name.clone()),
                    label: format!("Create {kind_label} {}", routine.name),
                    sql: ddl.trim_end().trim_end_matches(';').to_string(),
                });
            }
            Err(error) => {
                notify(
                    &mut on_progress,
                    "dump",
                    Some(kind_label),
                    Some(&routine.name),
                    index,
                    total_objects,
                    &format!("Skipped {kind_label} {}: {error}", routine.name),
                );
            }
        }
    }

    for trigger in &triggers {
        index += 1;
        notify(
            &mut on_progress,
            "dump",
            Some("trigger"),
            Some(&trigger.name),
            index,
            total_objects,
            &format!("Dumping trigger: {}", trigger.name),
        );
        match engine
            .get_ddl(schema, ObjectKind::Trigger, &trigger.name)
            .await
        {
            Ok(ddl) => {
                script.statements.push(DumpStatement {
                    object_kind: Some("trigger"),
                    object_name: Some(trigger.name.clone()),
                    label: format!("Create trigger {}", trigger.name),
                    sql: ddl.trim_end().trim_end_matches(';').to_string(),
                });
            }
            Err(error) => {
                notify(
                    &mut on_progress,
                    "dump",
                    Some("trigger"),
                    Some(&trigger.name),
                    index,
                    total_objects,
                    &format!("Skipped trigger {}: {error}", trigger.name),
                );
            }
        }
    }

    notify(
        &mut on_progress,
        "dump",
        None,
        None,
        total_objects,
        total_objects,
        &format!("Dump ready: {} statement(s)", script.statements.len()),
    );

    Ok(script)
}

fn push_database_preamble(
    script: &mut DumpScript,
    schema: &str,
    include_schema: bool,
    dialect: DumpDialect,
) {
    match dialect {
        DumpDialect::Mysql => {
            if include_schema {
                script.statements.push(DumpStatement {
                    object_kind: Some("database"),
                    object_name: Some(schema.to_string()),
                    label: format!("Create database {schema}"),
                    sql: format!("CREATE DATABASE IF NOT EXISTS {}", quote_ident(schema)),
                });
            }
            script.statements.push(DumpStatement {
                object_kind: Some("database"),
                object_name: Some(schema.to_string()),
                label: format!("Use database {schema}"),
                sql: format!("USE {}", quote_ident(schema)),
            });
        }
        DumpDialect::Postgres => {
            if include_schema {
                script.statements.push(DumpStatement {
                    object_kind: Some("schema"),
                    object_name: Some(schema.to_string()),
                    label: format!("Create schema {schema}"),
                    sql: format!("CREATE SCHEMA IF NOT EXISTS {}", quote_ident_pg(schema)),
                });
            } else {
                script.statements.push(DumpStatement {
                    object_kind: Some("schema"),
                    object_name: Some(schema.to_string()),
                    label: format!("Set search_path {schema}"),
                    sql: format!("SET search_path TO {}", quote_ident_pg(schema)),
                });
            }
        }
        DumpDialect::Mssql => {
            if include_schema {
                script.statements.push(DumpStatement {
                    object_kind: Some("database"),
                    object_name: Some(schema.to_string()),
                    label: format!("Create database {schema}"),
                    sql: format!(
                        "IF DB_ID(N'{}') IS NULL CREATE DATABASE {}",
                        schema.replace('\'', "''"),
                        quote_ident_mssql(schema)
                    ),
                });
            }
            script.statements.push(DumpStatement {
                object_kind: Some("database"),
                object_name: Some(schema.to_string()),
                label: format!("Use database {schema}"),
                sql: format!("USE {}", quote_ident_mssql(schema)),
            });
        }
        DumpDialect::Oracle => {
            if include_schema {
                if let Ok(statements) = crate::db::ident::create_oracle_schema_statements(schema) {
                    for (index, sql) in statements.into_iter().enumerate() {
                        script.statements.push(DumpStatement {
                            object_kind: Some("schema"),
                            object_name: Some(schema.to_string()),
                            label: if index == 0 {
                                format!("Create schema {schema}")
                            } else {
                                format!("Grant privileges on {schema}")
                            },
                            sql,
                        });
                    }
                }
            }
            script.statements.push(DumpStatement {
                object_kind: Some("schema"),
                object_name: Some(schema.to_string()),
                label: format!("Set current schema {schema}"),
                sql: format!(
                    "ALTER SESSION SET CURRENT_SCHEMA = {}",
                    quote_ident_pg(schema)
                ),
            });
        }
    }
}

async fn append_table_data_statements<F>(
    script: &mut DumpScript,
    engine: &impl DatabaseEngine,
    schema: &str,
    table: &str,
    dialect: DumpDialect,
    on_progress: &mut Option<F>,
    object_index: u32,
    object_total: u32,
) -> AppResult<()>
where
    F: FnMut(&str, Option<&str>, Option<&str>, u32, u32, &str),
{
    const BATCH: u32 = 200;
    const MAX_ROWS: u64 = 50_000;

    let target = match dialect {
        DumpDialect::Mysql => format!("{}.{}", quote_ident(schema), quote_ident(table)),
        DumpDialect::Postgres | DumpDialect::Oracle => {
            format!("{}.{}", quote_ident_pg(schema), quote_ident_pg(table))
        }
        DumpDialect::Mssql => qualify_mssql(schema, table),
    };

    let mut offset = 0_u64;
    let mut total = 0_u64;
    loop {
        if total >= MAX_ROWS {
            notify(
                on_progress,
                "dump",
                Some("table"),
                Some(table),
                object_index,
                object_total,
                &format!("Data truncated at {MAX_ROWS} rows for table {table}"),
            );
            break;
        }
        let limit = BATCH.min((MAX_ROWS - total) as u32);
        let result = engine.preview_table(schema, table, limit, offset).await?;
        if result.rows.is_empty() {
            break;
        }
        if result.columns.is_empty() {
            return Err(AppError::msg(format!(
                "table {table} returned no columns while dumping data"
            )));
        }

        let columns = result
            .columns
            .iter()
            .map(|column| match dialect {
                DumpDialect::Mysql => quote_ident(&column.name),
                DumpDialect::Postgres | DumpDialect::Oracle => quote_ident_pg(&column.name),
                DumpDialect::Mssql => quote_ident_mssql(&column.name),
            })
            .collect::<Vec<_>>()
            .join(", ");

        let mut sql = format!("INSERT INTO {target} ({columns}) VALUES\n");
        for (row_index, row) in result.rows.iter().enumerate() {
            let values = row
                .iter()
                .map(|value| sql_literal(value.as_deref()))
                .collect::<Vec<_>>()
                .join(", ");
            sql.push_str("  (");
            sql.push_str(&values);
            sql.push(')');
            if row_index + 1 == result.rows.len() {
                sql.push(';');
            } else {
                sql.push_str(",\n");
            }
        }

        let fetched = result.rows.len() as u64;
        let from_row = total + 1;
        let to_row = total + fetched;
        notify(
            on_progress,
            "dump",
            Some("table"),
            Some(table),
            object_index,
            object_total,
            &format!("Dumping rows {from_row}-{to_row} for table {table}"),
        );
        script.statements.push(DumpStatement {
            object_kind: Some("table"),
            object_name: Some(table.to_string()),
            label: format!("Insert into {table} (rows {from_row}-{to_row})"),
            sql: sql.trim_end_matches(';').to_string(),
        });

        total += fetched;
        offset += fetched;
        if fetched < u64::from(limit) || result.truncated {
            if result.truncated {
                notify(
                    on_progress,
                    "dump",
                    Some("table"),
                    Some(table),
                    object_index,
                    object_total,
                    &format!("Data truncated by preview limit for table {table}"),
                );
            }
            break;
        }
    }

    if total == 0 {
        notify(
            on_progress,
            "dump",
            Some("table"),
            Some(table),
            object_index,
            object_total,
            &format!("No rows in table {table}"),
        );
    }
    Ok(())
}

fn notify<F>(
    on_progress: &mut Option<F>,
    phase: &str,
    object_kind: Option<&str>,
    object_name: Option<&str>,
    current: u32,
    total: u32,
    message: &str,
) where
    F: FnMut(&str, Option<&str>, Option<&str>, u32, u32, &str),
{
    if let Some(callback) = on_progress.as_mut() {
        callback(phase, object_kind, object_name, current, total, message);
    }
}

fn sql_literal(value: Option<&str>) -> String {
    match value {
        None => "NULL".to_string(),
        Some(text) => format!("'{}'", text.replace('\'', "''")),
    }
}

pub fn rewrite_dump_schema_name(
    script: &mut DumpScript,
    dialect: DumpDialect,
    from: &str,
    to: &str,
) -> AppResult<()> {
    validate_ident(from)?;
    validate_ident(to)?;
    if from == to {
        return Ok(());
    }

    let (from_q, to_q) = match dialect {
        DumpDialect::Mysql => (quote_ident(from), quote_ident(to)),
        DumpDialect::Postgres | DumpDialect::Oracle => (quote_ident_pg(from), quote_ident_pg(to)),
        DumpDialect::Mssql => (quote_ident_mssql(from), quote_ident_mssql(to)),
    };

    for statement in &mut script.statements {
        statement.sql = statement.sql.replace(&from_q, &to_q);
        if let Some(name) = statement.object_name.as_mut() {
            if name == from {
                *name = to.to_string();
            }
        }
        statement.label = statement.label.replace(from, to);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_dump_modes() {
        assert_eq!(dump_mode_label(true, true), "schema + data");
        assert_eq!(dump_mode_label(true, false), "schema only");
        assert_eq!(dump_mode_label(false, true), "data only");
    }

    #[test]
    fn rewrites_quoted_schema_names() {
        let mut script = DumpScript {
            statements: vec![
                DumpStatement {
                    object_kind: Some("database"),
                    object_name: Some("shop".into()),
                    label: "Create database shop".into(),
                    sql: "CREATE DATABASE IF NOT EXISTS `shop`".into(),
                },
                DumpStatement {
                    object_kind: Some("table"),
                    object_name: Some("orders".into()),
                    label: "Insert into orders".into(),
                    sql: "INSERT INTO `shop`.`orders` (`id`) VALUES (1)".into(),
                },
            ],
        };
        rewrite_dump_schema_name(&mut script, DumpDialect::Mysql, "shop", "shop_copy").unwrap();
        assert!(script.statements[0].sql.contains("`shop_copy`"));
        assert!(script.statements[1].sql.contains("`shop_copy`.`orders`"));
        assert_eq!(
            script.statements[0].object_name.as_deref(),
            Some("shop_copy")
        );
    }
}
