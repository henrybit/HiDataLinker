use super::parse::ParseSample;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestedColumn {
    pub source_name: String,
    pub name: String,
    pub sql_type: String,
    pub nullable: bool,
}

pub fn infer_columns(sample: &ParseSample, engine: &str) -> Vec<SuggestedColumn> {
    let engine = engine.to_ascii_lowercase();
    sample
        .headers
        .iter()
        .enumerate()
        .map(|(i, header)| {
            let values: Vec<&str> = sample
                .sample_rows
                .iter()
                .filter_map(|row| row.get(i).map(String::as_str))
                .collect();
            let nulls = values.iter().filter(|v| v.is_empty()).count();
            let non_empty: Vec<&str> = values.into_iter().filter(|v| !v.is_empty()).collect();
            let sql_type = infer_sql_type(&non_empty, &engine);
            SuggestedColumn {
                source_name: header.clone(),
                name: normalize_ident(header),
                sql_type,
                nullable: nulls > 0 || non_empty.is_empty(),
            }
        })
        .collect()
}

pub fn build_create_table_ddl(
    engine: &str,
    schema: &str,
    table: &str,
    columns: &[SuggestedColumn],
) -> String {
    let engine = engine.to_ascii_lowercase();
    let cols = columns
        .iter()
        .map(|c| {
            let null_sql = if c.nullable { "NULL" } else { "NOT NULL" };
            format!(
                "  {} {} {}",
                quote_ident(&engine, &c.name),
                c.sql_type,
                null_sql
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let qualified = qualify_table(&engine, schema, table);
    format!("CREATE TABLE {qualified} (\n{cols}\n);")
}

pub fn quote_ident(engine: &str, ident: &str) -> String {
    match engine {
        "postgres" | "postgresql" => format!("\"{}\"", ident.replace('"', "\"\"")),
        _ => format!("`{}`", ident.replace('`', "``")),
    }
}

pub fn qualify_table(engine: &str, schema: &str, table: &str) -> String {
    let engine = engine.to_ascii_lowercase();
    if schema.trim().is_empty() {
        return quote_ident(&engine, table);
    }
    format!(
        "{}.{}",
        quote_ident(&engine, schema.trim()),
        quote_ident(&engine, table)
    )
}

pub fn normalize_ident(raw: &str) -> String {
    let mut out = String::new();
    for c in raw.trim().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        "col".into()
    } else if trimmed
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
    {
        format!("c_{trimmed}")
    } else {
        trimmed.to_string()
    }
}

fn infer_sql_type(values: &[&str], engine: &str) -> String {
    if values.is_empty() {
        return text_type(engine, 255);
    }
    if values.iter().all(|v| looks_bool(v)) {
        return if engine.starts_with("postgres") {
            "BOOLEAN".into()
        } else {
            "TINYINT(1)".into()
        };
    }
    if values.iter().all(|v| looks_int(v)) {
        let max_abs = values
            .iter()
            .filter_map(|v| v.parse::<i128>().ok())
            .map(|n| n.unsigned_abs())
            .max()
            .unwrap_or(0);
        return if max_abs > i32::MAX as u128 {
            "BIGINT".into()
        } else {
            "INT".into()
        };
    }
    if values.iter().all(|v| looks_decimal(v)) {
        return if engine.starts_with("postgres") {
            "NUMERIC(18,6)".into()
        } else {
            "DECIMAL(18,6)".into()
        };
    }
    if values.iter().all(|v| looks_date(v)) {
        return "DATE".into();
    }
    if values.iter().all(|v| looks_timestamp(v)) {
        return if engine.starts_with("postgres") {
            "TIMESTAMP".into()
        } else {
            "DATETIME".into()
        };
    }
    let max_len = values.iter().map(|v| v.chars().count()).max().unwrap_or(0);
    if max_len > 1000 {
        return if engine.starts_with("postgres") {
            "TEXT".into()
        } else {
            "LONGTEXT".into()
        };
    }
    let width = ((max_len + 15) / 16 * 16).clamp(32, 1024);
    text_type(engine, width)
}

fn text_type(engine: &str, width: usize) -> String {
    if engine.starts_with("postgres") {
        format!("VARCHAR({width})")
    } else {
        format!("VARCHAR({width})")
    }
}

fn looks_bool(v: &str) -> bool {
    matches!(
        v.to_ascii_lowercase().as_str(),
        "true" | "false" | "0" | "1" | "yes" | "no" | "y" | "n"
    )
}

fn looks_int(v: &str) -> bool {
    !v.is_empty() && v.parse::<i128>().is_ok()
}

fn looks_decimal(v: &str) -> bool {
    !v.is_empty() && v.parse::<f64>().is_ok()
}

fn looks_date(v: &str) -> bool {
    let re_ok = v.len() == 10
        && v.as_bytes().get(4) == Some(&b'-')
        && v.as_bytes().get(7) == Some(&b'-')
        && v.chars().filter(|c| c.is_ascii_digit()).count() == 8;
    re_ok
}

fn looks_timestamp(v: &str) -> bool {
    (v.contains('T') || v.contains(' '))
        && v.len() >= 16
        && v.chars().filter(|c| c.is_ascii_digit()).count() >= 12
}
