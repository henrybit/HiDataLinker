use super::{suggested_table_name, SAMPLE_ROW_LIMIT};
use crate::error::{AppError, AppResult};
use calamine::{open_workbook_auto, Data, Reader};
use encoding_rs::{Encoding, GBK, UTF_8};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use std::path::Path;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AttachmentKind {
    Csv,
    Xlsx,
    Xls,
}

impl AttachmentKind {
    pub fn from_name(name: &str) -> Option<Self> {
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".csv") {
            Some(Self::Csv)
        } else if lower.ends_with(".xlsx") {
            Some(Self::Xlsx)
        } else if lower.ends_with(".xls") {
            Some(Self::Xls)
        } else {
            None
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnStat {
    pub name: String,
    pub null_count: u64,
    pub sample_values: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseSample {
    pub headers: Vec<String>,
    pub sample_rows: Vec<Vec<String>>,
    pub total_rows: u64,
    pub column_count: usize,
    pub column_stats: Vec<ColumnStat>,
    pub suggested_table: String,
    pub sheet_name: Option<String>,
    pub encoding: Option<String>,
    pub delimiter: Option<String>,
}

pub fn parse_tabular_sample(path: &Path, file_name: &str) -> AppResult<ParseSample> {
    let kind = AttachmentKind::from_name(file_name)
        .or_else(|| AttachmentKind::from_name(&path.to_string_lossy()))
        .ok_or_else(|| AppError::msg("unsupported tabular file type"))?;
    match kind {
        AttachmentKind::Csv => parse_csv(path, file_name),
        AttachmentKind::Xlsx | AttachmentKind::Xls => parse_excel(path, file_name),
    }
}

fn parse_csv(path: &Path, file_name: &str) -> AppResult<ParseSample> {
    let mut file = fs::File::open(path)?;
    let mut raw = Vec::new();
    file.read_to_end(&mut raw)?;
    let (text, encoding_label) = decode_bytes(&raw);
    let delimiter = detect_delimiter(&text);
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter as u8)
        .flexible(true)
        .from_reader(text.as_bytes());

    let headers = reader
        .headers()
        .map_err(|e| AppError::msg(format!("csv header: {e}")))?
        .iter()
        .map(|h| h.trim().to_string())
        .collect::<Vec<_>>();
    if headers.is_empty() {
        return Err(AppError::msg("csv has no header row"));
    }
    let headers = uniquify_headers(headers);

    let mut sample_rows = Vec::new();
    let mut total_rows = 0u64;
    let mut null_counts = vec![0u64; headers.len()];
    let mut sample_values: Vec<Vec<String>> = vec![Vec::new(); headers.len()];

    for record in reader.records() {
        let record = record.map_err(|e| AppError::msg(format!("csv row: {e}")))?;
        total_rows += 1;
        let row: Vec<String> = (0..headers.len())
            .map(|i| record.get(i).unwrap_or("").trim().to_string())
            .collect();
        for (i, cell) in row.iter().enumerate() {
            if cell.is_empty() {
                null_counts[i] += 1;
            } else if sample_values[i].len() < 5 && !sample_values[i].contains(cell) {
                sample_values[i].push(cell.clone());
            }
        }
        if sample_rows.len() < SAMPLE_ROW_LIMIT {
            sample_rows.push(row);
        }
    }

    Ok(build_sample(
        headers,
        sample_rows,
        total_rows,
        null_counts,
        sample_values,
        file_name,
        None,
        Some(encoding_label),
        Some(delimiter.to_string()),
    ))
}

fn parse_excel(path: &Path, file_name: &str) -> AppResult<ParseSample> {
    let mut workbook = open_workbook_auto(path)
        .map_err(|e| AppError::msg(format!("open excel: {e}")))?;
    let sheet_names = workbook.sheet_names().to_vec();
    let sheet_name = sheet_names
        .into_iter()
        .find(|name| {
            workbook
                .worksheet_range(name)
                .ok()
                .map(|range| range.rows().next().is_some())
                .unwrap_or(false)
        })
        .ok_or_else(|| AppError::msg("excel workbook has no data sheet"))?;

    let range = workbook
        .worksheet_range(&sheet_name)
        .map_err(|e| AppError::msg(format!("read sheet: {e}")))?;

    let mut rows_iter = range.rows();
    let header_row = rows_iter
        .next()
        .ok_or_else(|| AppError::msg("excel sheet is empty"))?;
    let headers = uniquify_headers(
        header_row
            .iter()
            .map(cell_to_string)
            .map(|s| s.trim().to_string())
            .collect(),
    );
    if headers.iter().all(|h| h.is_empty()) {
        return Err(AppError::msg("excel header row is empty"));
    }

    let mut sample_rows = Vec::new();
    let mut total_rows = 0u64;
    let mut null_counts = vec![0u64; headers.len()];
    let mut sample_values: Vec<Vec<String>> = vec![Vec::new(); headers.len()];

    for row in rows_iter {
        let cells: Vec<String> = (0..headers.len())
            .map(|i| row.get(i).map(cell_to_string).unwrap_or_default())
            .map(|s| s.trim().to_string())
            .collect();
        if cells.iter().all(|c| c.is_empty()) {
            continue;
        }
        total_rows += 1;
        for (i, cell) in cells.iter().enumerate() {
            if cell.is_empty() {
                null_counts[i] += 1;
            } else if sample_values[i].len() < 5 && !sample_values[i].contains(cell) {
                sample_values[i].push(cell.clone());
            }
        }
        if sample_rows.len() < SAMPLE_ROW_LIMIT {
            sample_rows.push(cells);
        }
    }

    Ok(build_sample(
        headers,
        sample_rows,
        total_rows,
        null_counts,
        sample_values,
        file_name,
        Some(sheet_name),
        None,
        None,
    ))
}

fn build_sample(
    headers: Vec<String>,
    sample_rows: Vec<Vec<String>>,
    total_rows: u64,
    null_counts: Vec<u64>,
    sample_values: Vec<Vec<String>>,
    file_name: &str,
    sheet_name: Option<String>,
    encoding: Option<String>,
    delimiter: Option<String>,
) -> ParseSample {
    let column_stats = headers
        .iter()
        .enumerate()
        .map(|(i, name)| ColumnStat {
            name: name.clone(),
            null_count: null_counts.get(i).copied().unwrap_or(0),
            sample_values: sample_values.get(i).cloned().unwrap_or_default(),
        })
        .collect();
    let column_count = headers.len();
    ParseSample {
        headers,
        sample_rows,
        total_rows,
        column_count,
        column_stats,
        suggested_table: suggested_table_name(file_name),
        sheet_name,
        encoding,
        delimiter,
    }
}

fn cell_to_string(cell: &Data) -> String {
    match cell {
        Data::Empty => String::new(),
        Data::String(s) => s.clone(),
        Data::Float(f) => {
            if f.fract() == 0.0 && *f >= i64::MIN as f64 && *f <= i64::MAX as f64 {
                format!("{}", *f as i64)
            } else {
                f.to_string()
            }
        }
        Data::Int(i) => i.to_string(),
        Data::Bool(b) => b.to_string(),
        Data::DateTime(dt) => dt.to_string(),
        Data::DateTimeIso(s) | Data::DurationIso(s) => s.clone(),
        Data::Error(e) => format!("#ERR:{e:?}"),
    }
}

fn decode_bytes(raw: &[u8]) -> (String, String) {
    if let Ok(text) = std::str::from_utf8(raw) {
        return (text.to_string(), "utf-8".into());
    }
    // Prefer GBK for common CN CSVs when UTF-8 fails.
    for (enc, label) in [(GBK, "gbk"), (UTF_8, "utf-8")] {
        let (cow, _, had_errors) = enc.decode(raw);
        if !had_errors {
            return (cow.into_owned(), label.into());
        }
    }
    let (cow, _, _) = Encoding::for_label(b"gb18030")
        .unwrap_or(GBK)
        .decode(raw);
    (cow.into_owned(), "gb18030".into())
}

fn detect_delimiter(text: &str) -> char {
    let first_line = text.lines().next().unwrap_or("");
    let candidates = [',', ';', '\t'];
    let mut best = (',', 0usize);
    for d in candidates {
        let count = first_line.matches(d).count();
        if count > best.1 {
            best = (d, count);
        }
    }
    best.0
}

fn uniquify_headers(headers: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashMap::<String, usize>::new();
    headers
        .into_iter()
        .enumerate()
        .map(|(i, raw)| {
            let base = if raw.trim().is_empty() {
                format!("col_{}", i + 1)
            } else {
                raw.trim().to_string()
            };
            let key = base.to_ascii_lowercase();
            let n = seen.entry(key).or_insert(0);
            *n += 1;
            if *n == 1 {
                base
            } else {
                format!("{base}_{n}")
            }
        })
        .collect()
}
