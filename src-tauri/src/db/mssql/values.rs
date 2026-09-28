use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use rust_decimal::Decimal;
use tiberius::{ColumnType, Row};

pub fn cell_to_string(row: &Row, index: usize) -> Option<String> {
    let column_type = row.columns().get(index)?.column_type();
    match column_type {
        ColumnType::Bit | ColumnType::Bitn => row
            .try_get::<bool, _>(index)
            .ok()
            .flatten()
            .map(|value| if value { "1" } else { "0" }.to_string()),
        ColumnType::Int1 => num::<u8>(row, index),
        ColumnType::Int2 => num::<i16>(row, index),
        ColumnType::Int4 | ColumnType::Intn => {
            num::<i32>(row, index).or_else(|| num::<i64>(row, index))
        }
        ColumnType::Int8 => num::<i64>(row, index),
        ColumnType::Float4 | ColumnType::Floatn => {
            num::<f32>(row, index).or_else(|| num::<f64>(row, index))
        }
        ColumnType::Float8 => num::<f64>(row, index),
        ColumnType::Decimaln | ColumnType::Numericn | ColumnType::Money | ColumnType::Money4 => row
            .try_get::<Decimal, _>(index)
            .ok()
            .flatten()
            .map(|value| value.to_string()),
        ColumnType::Datetime
        | ColumnType::Datetime2
        | ColumnType::Datetime4
        | ColumnType::Datetimen => row
            .try_get::<NaiveDateTime, _>(index)
            .ok()
            .flatten()
            .map(|value| value.format("%Y-%m-%d %H:%M:%S%.f").to_string()),
        ColumnType::Daten => row
            .try_get::<NaiveDate, _>(index)
            .ok()
            .flatten()
            .map(|value| value.to_string()),
        ColumnType::Timen => row
            .try_get::<NaiveTime, _>(index)
            .ok()
            .flatten()
            .map(|value| value.format("%H:%M:%S%.f").to_string()),
        ColumnType::DatetimeOffsetn => row
            .try_get::<chrono::DateTime<chrono::FixedOffset>, _>(index)
            .ok()
            .flatten()
            .map(|value| value.format("%Y-%m-%d %H:%M:%S%.f %:z").to_string()),
        ColumnType::Guid => text(row, index),
        ColumnType::BigBinary | ColumnType::BigVarBin | ColumnType::Image => {
            row.try_get::<&[u8], _>(index).ok().flatten().map(to_hex)
        }
        _ => text(row, index),
    }
}

fn num<T>(row: &Row, index: usize) -> Option<String>
where
    T: ToString + for<'a> tiberius::FromSql<'a>,
{
    row.try_get::<T, _>(index)
        .ok()
        .flatten()
        .map(|value| value.to_string())
}

fn text(row: &Row, index: usize) -> Option<String> {
    if let Ok(Some(value)) = row.try_get::<&str, _>(index) {
        return Some(value.to_string());
    }
    if let Ok(Some(value)) = row.try_get::<Decimal, _>(index) {
        return Some(value.to_string());
    }
    if let Ok(Some(value)) = row.try_get::<i64, _>(index) {
        return Some(value.to_string());
    }
    if let Ok(Some(value)) = row.try_get::<f64, _>(index) {
        return Some(value.to_string());
    }
    None
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

pub fn parse_u64(value: Option<String>) -> Option<u64> {
    let text = value?;
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if let Ok(value) = text.parse::<u64>() {
        return Some(value);
    }
    if let Some((whole, frac)) = text.split_once('.') {
        if frac.chars().all(|ch| ch == '0') {
            return whole.parse().ok();
        }
    }
    text.parse::<f64>().ok().map(|value| value.max(0.0) as u64)
}

pub fn parse_bool(value: Option<String>) -> bool {
    matches!(
        value.as_deref().map(str::trim),
        Some("1") | Some("true") | Some("TRUE")
    )
}
