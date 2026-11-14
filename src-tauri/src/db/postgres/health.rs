//! Schema health check: catalog-only rules based on the PostgreSQL wiki's
//! "Don't Do This" page and common index maintenance advice.

use super::{pg_literal, qualify, quote_ident};
use crate::error::AppResult;
use crate::models::{HealthFinding, HealthSeverity, SchemaHealthReport};
use deadpool_postgres::{Object, Pool};
use std::time::Instant;

const RULES: &[&str] = &[
    "timestamp_without_time_zone",
    "char_n",
    "varchar_n",
    "money_type",
    "timetz_type",
    "json_type",
    "serial_column",
    "missing_primary_key",
    "unindexed_foreign_key",
    "duplicate_index",
    "invalid_index",
    "unused_index",
    "rule_usage",
    "table_inheritance",
    "uppercase_identifier",
    "sequence_exhaustion",
    "sql_ascii_encoding",
];

/// Matches relations `c` that are not owned by an extension.
const NOT_EXTENSION_RELATION: &str = "NOT EXISTS (
    SELECT 1 FROM pg_depend ext
    WHERE ext.classid = 'pg_class'::regclass AND ext.objid = c.oid AND ext.deptype = 'e'
)";

struct Findings(Vec<HealthFinding>);

impl Findings {
    fn push(
        &mut self,
        rule: &str,
        severity: HealthSeverity,
        object: String,
        facts: String,
        fix_sql: Option<String>,
    ) {
        self.0.push(HealthFinding {
            rule: rule.to_string(),
            severity,
            object,
            facts,
            fix_sql,
        });
    }
}

pub(super) async fn schema_health(pool: &Pool, schema: &str) -> AppResult<SchemaHealthReport> {
    let started = Instant::now();
    let client = pool.get().await?;
    let mut findings = Findings(Vec::new());

    column_types(&client, schema, &mut findings).await?;
    serial_columns(&client, schema, &mut findings).await?;
    missing_primary_keys(&client, schema, &mut findings).await?;
    unindexed_foreign_keys(&client, schema, &mut findings).await?;
    duplicate_indexes(&client, schema, &mut findings).await?;
    invalid_indexes(&client, schema, &mut findings).await?;
    unused_indexes(&client, schema, &mut findings).await?;
    rules_and_inheritance(&client, schema, &mut findings).await?;
    uppercase_identifiers(&client, schema, &mut findings).await?;
    sequence_exhaustion(&client, schema, &mut findings).await?;
    database_encoding(&client, &mut findings).await?;

    let stats_reset = client
        .query_one(
            "SELECT stats_reset::text FROM pg_stat_database WHERE datname = current_database()",
            &[],
        )
        .await?
        .get(0);

    let mut findings = findings.0;
    findings
        .sort_by(|a, b| (a.severity, &a.rule, &a.object).cmp(&(b.severity, &b.rule, &b.object)));
    Ok(SchemaHealthReport {
        findings,
        rules_checked: RULES.iter().map(|rule| rule.to_string()).collect(),
        stats_reset,
        duration_ms: started.elapsed().as_millis() as u64,
    })
}

async fn column_types(client: &Object, schema: &str, out: &mut Findings) -> AppResult<()> {
    let sql = format!(
        "SELECT c.relname, a.attname, t.typname,
                pg_catalog.format_type(a.atttypid, a.atttypmod), a.atttypmod
         FROM pg_attribute a
         JOIN pg_class c ON c.oid = a.attrelid
         JOIN pg_namespace n ON n.oid = c.relnamespace
         JOIN pg_type t ON t.oid = a.atttypid
         WHERE n.nspname = $1
           AND c.relkind IN ('r', 'p') AND NOT c.relispartition
           AND a.attnum > 0 AND NOT a.attisdropped
           AND t.typname IN ('timestamp', 'bpchar', 'varchar', 'money', 'timetz', 'json')
           AND {NOT_EXTENSION_RELATION}
         ORDER BY c.relname, a.attnum"
    );
    for row in client.query(&sql, &[&schema]).await? {
        let table: String = row.get(0);
        let column: String = row.get(1);
        let type_name: String = row.get(2);
        let formatted: String = row.get(3);
        let typmod: i32 = row.get(4);
        let object = format!("{table}.{column}");
        let alter = |target: &str, using: Option<String>| {
            let mut sql = format!(
                "ALTER TABLE {} ALTER COLUMN {} TYPE {target}",
                qualify(schema, &table),
                quote_ident(&column)
            );
            if let Some(using) = using {
                sql.push_str(&format!(" USING {using}"));
            }
            sql.push(';');
            sql
        };
        let quoted = quote_ident(&column);
        match type_name.as_str() {
            "timestamp" => {
                let target = if typmod >= 0 {
                    format!("timestamptz({typmod})")
                } else {
                    "timestamptz".to_string()
                };
                out.push(
                    "timestamp_without_time_zone",
                    HealthSeverity::Warning,
                    object,
                    formatted,
                    Some(alter(&target, Some(format!("{quoted} AT TIME ZONE 'UTC'")))),
                );
            }
            "bpchar" => out.push(
                "char_n",
                HealthSeverity::Warning,
                object,
                formatted,
                Some(alter("text", None)),
            ),
            "varchar" if typmod > 0 => out.push(
                "varchar_n",
                HealthSeverity::Info,
                object,
                formatted,
                Some(alter("text", None)),
            ),
            "money" => out.push(
                "money_type",
                HealthSeverity::Warning,
                object,
                formatted,
                Some(alter("numeric(19, 4)", Some(format!("{quoted}::numeric")))),
            ),
            "timetz" => out.push(
                "timetz_type",
                HealthSeverity::Warning,
                object,
                formatted,
                None,
            ),
            "json" => out.push(
                "json_type",
                HealthSeverity::Info,
                object,
                formatted,
                Some(alter("jsonb", Some(format!("{quoted}::jsonb")))),
            ),
            _ => {}
        }
    }
    Ok(())
}

async fn serial_columns(client: &Object, schema: &str, out: &mut Findings) -> AppResult<()> {
    let sql = format!(
        "SELECT c.relname, a.attname, sn.nspname, s.relname,
                CASE WHEN has_sequence_privilege(s.oid, 'SELECT,USAGE')
                     THEN pg_sequence_last_value(s.oid) END
         FROM pg_depend d
         JOIN pg_class s ON s.oid = d.objid AND s.relkind = 'S'
         JOIN pg_namespace sn ON sn.oid = s.relnamespace
         JOIN pg_class c ON c.oid = d.refobjid
         JOIN pg_namespace n ON n.oid = c.relnamespace
         JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum = d.refobjsubid
         WHERE d.classid = 'pg_class'::regclass AND d.refclassid = 'pg_class'::regclass
           AND d.deptype = 'a'
           AND n.nspname = $1 AND c.relkind IN ('r', 'p') AND NOT c.relispartition
           AND {NOT_EXTENSION_RELATION}
         ORDER BY c.relname, a.attnum"
    );
    for row in client.query(&sql, &[&schema]).await? {
        let table: String = row.get(0);
        let column: String = row.get(1);
        let sequence_schema: String = row.get(2);
        let sequence: String = row.get(3);
        let last_value: Option<i64> = row.get(4);
        let qualified = qualify(schema, &table);
        let quoted = quote_ident(&column);
        let mut fix = format!(
            "BEGIN;\n\
             ALTER TABLE {qualified} ALTER COLUMN {quoted} DROP DEFAULT;\n\
             DROP SEQUENCE {};\n\
             ALTER TABLE {qualified} ALTER COLUMN {quoted} ADD GENERATED BY DEFAULT AS IDENTITY;\n",
            qualify(&sequence_schema, &sequence)
        );
        if let Some(value) = last_value {
            fix.push_str(&format!(
                "SELECT pg_catalog.setval(pg_catalog.pg_get_serial_sequence({}, {}), {value}, true);\n",
                pg_literal(&qualified),
                pg_literal(&column)
            ));
        }
        fix.push_str("COMMIT;");
        out.push(
            "serial_column",
            HealthSeverity::Info,
            format!("{table}.{column}"),
            sequence,
            Some(fix),
        );
    }
    Ok(())
}

async fn missing_primary_keys(client: &Object, schema: &str, out: &mut Findings) -> AppResult<()> {
    let sql = format!(
        "SELECT c.relname, c.relkind::text,
                (SELECT i.relname
                 FROM pg_index x
                 JOIN pg_class i ON i.oid = x.indexrelid
                 WHERE x.indrelid = c.oid AND x.indisunique AND x.indisvalid
                   AND x.indpred IS NULL AND x.indexprs IS NULL
                   AND NOT EXISTS (
                       SELECT 1 FROM pg_attribute a
                       WHERE a.attrelid = c.oid AND a.attnum = ANY (x.indkey) AND NOT a.attnotnull
                   )
                 ORDER BY x.indnatts, i.relname
                 LIMIT 1)
         FROM pg_class c
         JOIN pg_namespace n ON n.oid = c.relnamespace
         WHERE n.nspname = $1 AND c.relkind IN ('r', 'p') AND NOT c.relispartition
           AND NOT EXISTS (SELECT 1 FROM pg_constraint p WHERE p.conrelid = c.oid AND p.contype = 'p')
           AND {NOT_EXTENSION_RELATION}
         ORDER BY c.relname"
    );
    for row in client.query(&sql, &[&schema]).await? {
        let table: String = row.get(0);
        let relkind: String = row.get(1);
        let candidate: Option<String> = row.get(2);
        let fix = candidate.as_ref().filter(|_| relkind == "r").map(|index| {
            format!(
                "ALTER TABLE {} ADD CONSTRAINT {} PRIMARY KEY USING INDEX {};",
                qualify(schema, &table),
                quote_ident(index),
                quote_ident(index)
            )
        });
        out.push(
            "missing_primary_key",
            HealthSeverity::Warning,
            table,
            candidate.unwrap_or_default(),
            fix,
        );
    }
    Ok(())
}

async fn unindexed_foreign_keys(
    client: &Object,
    schema: &str,
    out: &mut Findings,
) -> AppResult<()> {
    let sql = format!(
        "SELECT c.relname, co.conname, c.relkind::text,
                (SELECT string_agg(quote_ident(a.attname), ', ' ORDER BY k.ord)
                 FROM unnest(co.conkey) WITH ORDINALITY AS k(attnum, ord)
                 JOIN pg_attribute a ON a.attrelid = co.conrelid AND a.attnum = k.attnum),
                pg_size_pretty(pg_total_relation_size(c.oid))
         FROM pg_constraint co
         JOIN pg_class c ON c.oid = co.conrelid
         JOIN pg_namespace n ON n.oid = c.relnamespace
         WHERE n.nspname = $1 AND co.contype = 'f' AND co.conparentid = 0
           AND {NOT_EXTENSION_RELATION}
           AND NOT EXISTS (
               SELECT 1 FROM pg_index x
               WHERE x.indrelid = co.conrelid AND x.indpred IS NULL
                 AND (string_to_array(x.indkey::text, ' ')::int2[])[1:cardinality(co.conkey)] @> co.conkey
                 AND (string_to_array(x.indkey::text, ' ')::int2[])[1:cardinality(co.conkey)] <@ co.conkey
           )
         ORDER BY c.relname, co.conname"
    );
    for row in client.query(&sql, &[&schema]).await? {
        let table: String = row.get(0);
        let constraint: String = row.get(1);
        let relkind: String = row.get(2);
        let columns: String = row.get(3);
        let size: String = row.get(4);
        let concurrently = if relkind == "p" { "" } else { " CONCURRENTLY" };
        out.push(
            "unindexed_foreign_key",
            HealthSeverity::Warning,
            format!("{table}.{constraint}"),
            format!("({columns}) · {size}"),
            Some(format!(
                "CREATE INDEX{concurrently} ON {} ({columns});",
                qualify(schema, &table)
            )),
        );
    }
    Ok(())
}

async fn duplicate_indexes(client: &Object, schema: &str, out: &mut Findings) -> AppResult<()> {
    let sql = format!(
        "SELECT c.relname,
                array_agg(i.relname::text ORDER BY (con.oid IS NOT NULL) DESC, x.indisunique DESC, i.relname),
                array_agg(con.conname::text ORDER BY (con.oid IS NOT NULL) DESC, x.indisunique DESC, i.relname),
                array_agg(pg_size_pretty(pg_relation_size(i.oid)) ORDER BY (con.oid IS NOT NULL) DESC, x.indisunique DESC, i.relname)
         FROM pg_index x
         JOIN pg_class i ON i.oid = x.indexrelid
         JOIN pg_class c ON c.oid = x.indrelid
         JOIN pg_namespace n ON n.oid = c.relnamespace
         LEFT JOIN pg_constraint con ON con.conindid = x.indexrelid AND con.conrelid = x.indrelid
         WHERE n.nspname = $1 AND x.indisvalid AND {NOT_EXTENSION_RELATION}
         GROUP BY c.relname, x.indrelid, i.relam, x.indkey::text, x.indclass::text,
                  x.indcollation::text, x.indoption::text,
                  COALESCE(pg_get_expr(x.indexprs, x.indrelid), ''),
                  COALESCE(pg_get_expr(x.indpred, x.indrelid), '')
         HAVING count(*) > 1
         ORDER BY c.relname"
    );
    for row in client.query(&sql, &[&schema]).await? {
        let table: String = row.get(0);
        let indexes: Vec<String> = row.get(1);
        let constraints: Vec<Option<String>> = row.get(2);
        let sizes: Vec<String> = row.get(3);
        let kept = &indexes[0];
        for ((index, constraint), size) in indexes.iter().zip(&constraints).zip(&sizes).skip(1) {
            let fix = match constraint {
                Some(constraint) => format!(
                    "ALTER TABLE {} DROP CONSTRAINT {};",
                    qualify(schema, &table),
                    quote_ident(constraint)
                ),
                None => format!("DROP INDEX CONCURRENTLY {};", qualify(schema, index)),
            };
            out.push(
                "duplicate_index",
                HealthSeverity::Warning,
                format!("{table}.{index}"),
                format!("= {kept} · {size}"),
                Some(fix),
            );
        }
    }
    Ok(())
}

async fn invalid_indexes(client: &Object, schema: &str, out: &mut Findings) -> AppResult<()> {
    let sql = format!(
        "SELECT i.relname, c.relname, i.relkind::text
         FROM pg_index x
         JOIN pg_class i ON i.oid = x.indexrelid
         JOIN pg_class c ON c.oid = x.indrelid
         JOIN pg_namespace n ON n.oid = c.relnamespace
         WHERE n.nspname = $1 AND NOT x.indisvalid AND {NOT_EXTENSION_RELATION}
         ORDER BY i.relname"
    );
    for row in client.query(&sql, &[&schema]).await? {
        let index: String = row.get(0);
        let table: String = row.get(1);
        let relkind: String = row.get(2);
        let fix = (relkind == "i")
            .then(|| format!("REINDEX INDEX CONCURRENTLY {};", qualify(schema, &index)));
        out.push("invalid_index", HealthSeverity::Error, index, table, fix);
    }
    Ok(())
}

async fn unused_indexes(client: &Object, schema: &str, out: &mut Findings) -> AppResult<()> {
    let sql = format!(
        "SELECT s.indexrelname, s.relname, pg_size_pretty(pg_relation_size(s.indexrelid))
         FROM pg_stat_user_indexes s
         JOIN pg_index x ON x.indexrelid = s.indexrelid
         JOIN pg_class c ON c.oid = s.relid
         JOIN pg_class i ON i.oid = s.indexrelid
         WHERE s.schemaname = $1 AND s.idx_scan = 0
           AND x.indisvalid AND NOT x.indisunique AND NOT x.indisprimary
           AND i.relkind = 'i' AND NOT i.relispartition
           AND NOT EXISTS (SELECT 1 FROM pg_constraint con WHERE con.conindid = s.indexrelid)
           AND {NOT_EXTENSION_RELATION}
         ORDER BY pg_relation_size(s.indexrelid) DESC, s.indexrelname"
    );
    for row in client.query(&sql, &[&schema]).await? {
        let index: String = row.get(0);
        let table: String = row.get(1);
        let size: String = row.get(2);
        out.push(
            "unused_index",
            HealthSeverity::Info,
            index.clone(),
            format!("{table} · {size}"),
            Some(format!(
                "DROP INDEX CONCURRENTLY {};",
                qualify(schema, &index)
            )),
        );
    }
    Ok(())
}

async fn rules_and_inheritance(client: &Object, schema: &str, out: &mut Findings) -> AppResult<()> {
    for row in client
        .query(
            "SELECT tablename::text, rulename::text FROM pg_rules
             WHERE schemaname = $1 ORDER BY tablename, rulename",
            &[&schema],
        )
        .await?
    {
        let table: String = row.get(0);
        let rule: String = row.get(1);
        out.push(
            "rule_usage",
            HealthSeverity::Warning,
            format!("{table}.{rule}"),
            String::new(),
            None,
        );
    }
    let sql = format!(
        "SELECT c.relname, pn.nspname || '.' || p.relname
         FROM pg_inherits inh
         JOIN pg_class c ON c.oid = inh.inhrelid
         JOIN pg_namespace n ON n.oid = c.relnamespace
         JOIN pg_class p ON p.oid = inh.inhparent
         JOIN pg_namespace pn ON pn.oid = p.relnamespace
         WHERE n.nspname = $1 AND c.relkind = 'r' AND NOT c.relispartition
           AND {NOT_EXTENSION_RELATION}
         ORDER BY c.relname"
    );
    for row in client.query(&sql, &[&schema]).await? {
        out.push(
            "table_inheritance",
            HealthSeverity::Info,
            row.get(0),
            row.get(1),
            None,
        );
    }
    Ok(())
}

async fn uppercase_identifiers(client: &Object, schema: &str, out: &mut Findings) -> AppResult<()> {
    let relations = format!(
        "SELECT c.relname, c.relkind::text,
                EXISTS (SELECT 1 FROM pg_class o
                        WHERE o.relnamespace = c.relnamespace AND o.relname = lower(c.relname))
         FROM pg_class c
         JOIN pg_namespace n ON n.oid = c.relnamespace
         WHERE n.nspname = $1 AND c.relkind IN ('r', 'p', 'v', 'm', 'f')
           AND c.relname <> lower(c.relname) AND {NOT_EXTENSION_RELATION}
         ORDER BY c.relname"
    );
    for row in client.query(&relations, &[&schema]).await? {
        let name: String = row.get(0);
        let relkind: String = row.get(1);
        let taken: bool = row.get(2);
        let keyword = match relkind.as_str() {
            "v" => "VIEW",
            "m" => "MATERIALIZED VIEW",
            "f" => "FOREIGN TABLE",
            _ => "TABLE",
        };
        let fix = (!taken).then(|| {
            format!(
                "ALTER {keyword} {} RENAME TO {};",
                qualify(schema, &name),
                quote_ident(&name.to_lowercase())
            )
        });
        out.push(
            "uppercase_identifier",
            HealthSeverity::Info,
            name,
            String::new(),
            fix,
        );
    }
    let columns = format!(
        "SELECT c.relname, a.attname,
                EXISTS (SELECT 1 FROM pg_attribute o
                        WHERE o.attrelid = c.oid AND o.attname = lower(a.attname)
                          AND NOT o.attisdropped)
         FROM pg_attribute a
         JOIN pg_class c ON c.oid = a.attrelid
         JOIN pg_namespace n ON n.oid = c.relnamespace
         WHERE n.nspname = $1 AND c.relkind IN ('r', 'p') AND NOT c.relispartition
           AND a.attnum > 0 AND NOT a.attisdropped
           AND a.attname <> lower(a.attname) AND {NOT_EXTENSION_RELATION}
         ORDER BY c.relname, a.attnum"
    );
    for row in client.query(&columns, &[&schema]).await? {
        let table: String = row.get(0);
        let column: String = row.get(1);
        let taken: bool = row.get(2);
        let fix = (!taken).then(|| {
            format!(
                "ALTER TABLE {} RENAME COLUMN {} TO {};",
                qualify(schema, &table),
                quote_ident(&column),
                quote_ident(&column.to_lowercase())
            )
        });
        out.push(
            "uppercase_identifier",
            HealthSeverity::Info,
            format!("{table}.{column}"),
            String::new(),
            fix,
        );
    }
    Ok(())
}

async fn sequence_exhaustion(client: &Object, schema: &str, out: &mut Findings) -> AppResult<()> {
    let sql = format!(
        "SELECT c.relname, pg_sequence_last_value(c.oid), s.seqmax,
                pg_catalog.format_type(s.seqtypid, NULL),
                tc.relname, a.attname, at.typname, dep.deptype::text
         FROM pg_sequence s
         JOIN pg_class c ON c.oid = s.seqrelid
         JOIN pg_namespace n ON n.oid = c.relnamespace
         LEFT JOIN pg_depend dep
           ON dep.classid = 'pg_class'::regclass AND dep.objid = c.oid
          AND dep.refclassid = 'pg_class'::regclass AND dep.deptype IN ('a', 'i')
         LEFT JOIN pg_class tc ON tc.oid = dep.refobjid
         LEFT JOIN pg_attribute a ON a.attrelid = dep.refobjid AND a.attnum = dep.refobjsubid
         LEFT JOIN pg_type at ON at.oid = a.atttypid
         WHERE n.nspname = $1 AND s.seqincrement > 0
           AND CASE WHEN c.relkind = 'S' THEN has_sequence_privilege(c.oid, 'SELECT,USAGE') END
           AND {NOT_EXTENSION_RELATION}
         ORDER BY c.relname"
    );
    for row in client.query(&sql, &[&schema]).await? {
        let sequence: String = row.get(0);
        let Some(last_value) = row.get::<_, Option<i64>>(1) else {
            continue;
        };
        let seq_max: i64 = row.get(2);
        let seq_type: String = row.get(3);
        let table: Option<String> = row.get(4);
        let column: Option<String> = row.get(5);
        let column_type: Option<String> = row.get(6);
        let deptype: Option<String> = row.get(7);
        let column_max = match column_type.as_deref() {
            Some("int2") => i64::from(i16::MAX),
            Some("int4") => i64::from(i32::MAX),
            _ => i64::MAX,
        };
        let limit = seq_max.min(column_max);
        let used = last_value as f64 / limit as f64;
        if used < 0.75 {
            continue;
        }
        let severity = if used >= 0.9 {
            HealthSeverity::Error
        } else {
            HealthSeverity::Warning
        };
        let mut fix = Vec::new();
        if let (Some(table), Some(column)) = (&table, &column) {
            if column_max < i64::MAX {
                fix.push(format!(
                    "ALTER TABLE {} ALTER COLUMN {} TYPE bigint;",
                    qualify(schema, table),
                    quote_ident(column)
                ));
            }
        }
        if deptype.as_deref() != Some("i") && seq_max < i64::MAX {
            fix.push(format!(
                "ALTER SEQUENCE {} AS bigint MAXVALUE 9223372036854775807;",
                qualify(schema, &sequence)
            ));
        }
        let owner = match (&table, &column) {
            (Some(table), Some(column)) => format!(" → {table}.{column}"),
            _ => String::new(),
        };
        out.push(
            "sequence_exhaustion",
            severity,
            sequence,
            format!(
                "{:.1}% · {last_value} / {limit} · {seq_type}{owner}",
                used * 100.0
            ),
            (!fix.is_empty()).then(|| fix.join("\n")),
        );
    }
    Ok(())
}

async fn database_encoding(client: &Object, out: &mut Findings) -> AppResult<()> {
    let row = client
        .query_one(
            "SELECT current_database()::text, pg_encoding_to_char(encoding)::text
             FROM pg_database WHERE datname = current_database()",
            &[],
        )
        .await?;
    let encoding: String = row.get(1);
    if encoding == "SQL_ASCII" {
        out.push(
            "sql_ascii_encoding",
            HealthSeverity::Error,
            row.get(0),
            encoding,
            None,
        );
    }
    Ok(())
}
