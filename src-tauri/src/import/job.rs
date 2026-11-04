use super::infer::{qualify_table, quote_ident};
use super::parse::{parse_tabular_sample, AttachmentKind};
use super::INSERT_BATCH_SIZE;
use crate::db::{DatabaseEngine, LiveEngine};
use crate::error::{AppError, AppResult};
use crate::runtime::handle;
use crate::state::AppState;
use calamine::Data;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tauri::{AppHandle, Emitter, Manager};

pub const IMPORT_PROGRESS_EVENT: &str = "import-progress";
pub const IMPORT_DONE_EVENT: &str = "import-done";
pub const IMPORT_ERROR_EVENT: &str = "import-error";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportJobProgress {
    pub job_id: String,
    pub phase: String,
    pub written_rows: u64,
    pub total_rows: u64,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportJobStatus {
    pub job_id: String,
    pub status: String,
    pub written_rows: u64,
    pub total_rows: u64,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportStartRequest {
    pub job_id: String,
    pub script_dir: String,
}

struct LiveJob {
    cancelled: Arc<AtomicBool>,
    status: Mutex<ImportJobStatus>,
}

fn jobs() -> &'static Mutex<HashMap<String, Arc<LiveJob>>> {
    static JOBS: OnceLock<Mutex<HashMap<String, Arc<LiveJob>>>> = OnceLock::new();
    JOBS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn import_job_status(job_id: &str) -> Option<ImportJobStatus> {
    let map = jobs().lock().ok()?;
    let job = map.get(job_id)?;
    job.status.lock().ok().map(|s| s.clone())
}

pub fn cancel_import(job_id: &str) -> AppResult<()> {
    let map = jobs()
        .lock()
        .map_err(|_| AppError::msg("import job map poisoned"))?;
    if let Some(job) = map.get(job_id) {
        job.cancelled.store(true, Ordering::SeqCst);
        if let Ok(mut status) = job.status.lock() {
            if status.status == "running" {
                status.status = "cancelling".into();
            }
        }
        Ok(())
    } else {
        Err(AppError::msg("import job not found"))
    }
}

pub fn start_import(app: AppHandle, request: ImportStartRequest) -> AppResult<ImportJobStatus> {
    let script_dir = PathBuf::from(request.script_dir.trim());
    let manifest_path = script_dir.join("manifest.json");
    if !manifest_path.is_file() {
        return Err(AppError::msg("import manifest not found"));
    }
    let manifest: Value = serde_json::from_slice(&fs::read(&manifest_path)?)?;
    let job_id = request.job_id.trim().to_string();
    if job_id.is_empty() {
        return Err(AppError::msg("jobId is required"));
    }

    let total_rows = manifest
        .get("totalRows")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let initial = ImportJobStatus {
        job_id: job_id.clone(),
        status: "running".into(),
        written_rows: 0,
        total_rows,
        error: None,
    };
    let cancelled = Arc::new(AtomicBool::new(false));
    let live = Arc::new(LiveJob {
        cancelled: Arc::clone(&cancelled),
        status: Mutex::new(initial.clone()),
    });
    {
        let mut map = jobs()
            .lock()
            .map_err(|_| AppError::msg("import job map poisoned"))?;
        map.insert(job_id.clone(), Arc::clone(&live));
    }

    let app_bg = app.clone();
    let script_dir_bg = script_dir.clone();
    let job_id_bg = job_id.clone();
    handle().spawn(async move {
        let result =
            run_import_job(app_bg.clone(), script_dir_bg, job_id_bg.clone(), live).await;
        if let Err(error) = result {
            let message = error.user_message();
            let _ = app_bg.emit(
                IMPORT_ERROR_EVENT,
                json!({
                    "jobId": job_id_bg,
                    "error": message,
                }),
            );
        }
    });

    Ok(initial)
}

async fn run_import_job(
    app: AppHandle,
    script_dir: PathBuf,
    job_id: String,
    live: Arc<LiveJob>,
) -> AppResult<()> {
    let emit_progress = |phase: &str, written: u64, total: u64, message: &str| {
        if let Ok(mut status) = live.status.lock() {
            status.written_rows = written;
            status.total_rows = total;
            status.status = match phase {
                "done" => "done".into(),
                "cancelled" => "cancelled".into(),
                "error" => "error".into(),
                _ => "running".into(),
            };
            if phase == "error" {
                status.error = Some(message.to_string());
            }
        }
        let _ = app.emit(
            IMPORT_PROGRESS_EVENT,
            ImportJobProgress {
                job_id: job_id.clone(),
                phase: phase.into(),
                written_rows: written,
                total_rows: total,
                message: message.into(),
            },
        );
    };

    let manifest: Value =
        serde_json::from_slice(&fs::read(script_dir.join("manifest.json"))?)?;
    let plan: Value = serde_json::from_slice(&fs::read(script_dir.join("plan.json"))?)?;

    let connection_id = manifest
        .get("connectionId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let engine = manifest
        .get("engine")
        .and_then(|v| v.as_str())
        .unwrap_or("mysql")
        .to_ascii_lowercase();
    let schema = manifest
        .get("schema")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let table = manifest
        .get("table")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let attachment_path = manifest
        .get("attachmentPath")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let attachment_name = manifest
        .get("attachmentName")
        .and_then(|v| v.as_str())
        .unwrap_or("file")
        .to_string();
    let create_table = plan
        .get("createTable")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let ddl = plan
        .get("ddl")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let mappings = manifest
        .get("columnMappings")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let total_rows = manifest
        .get("totalRows")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    if connection_id.is_empty() || table.is_empty() {
        return Err(AppError::msg("manifest missing connectionId/table"));
    }

    emit_progress("prepare", 0, total_rows, "Preparing import");

    let state = app.state::<AppState>();
    let db_engine = state.engine(&connection_id)?;

    if create_table {
        if live.cancelled.load(Ordering::SeqCst) {
            emit_progress("cancelled", 0, total_rows, "Cancelled");
            return Ok(());
        }
        emit_progress("ddl", 0, total_rows, "Creating table");
        db_engine
            .execute_sql(Some(schema.as_str()), &ddl)
            .await?;
    }

    emit_progress("load", 0, total_rows, "Loading rows");

    let path = PathBuf::from(&attachment_path);
    let kind = AttachmentKind::from_name(&attachment_name)
        .ok_or_else(|| AppError::msg("unsupported attachment for import"))?;

    let sample = parse_tabular_sample(&path, &attachment_name)?;
    let header_index: HashMap<String, usize> = sample
        .headers
        .iter()
        .enumerate()
        .map(|(i, h)| (h.clone(), i))
        .collect();

    let mapped: Vec<(usize, String)> = mappings
        .iter()
        .filter_map(|m| {
            let source = m.get("source")?.as_str()?;
            let target = m.get("target")?.as_str()?;
            let idx = *header_index.get(source)?;
            Some((idx, target.to_string()))
        })
        .collect();
    if mapped.is_empty() {
        return Err(AppError::msg("no column mappings"));
    }

    let target_cols = mapped
        .iter()
        .map(|(_, t)| quote_ident(&engine, t))
        .collect::<Vec<_>>()
        .join(", ");
    let qualified = qualify_table(&engine, &schema, &table);

    let mut written = 0u64;
    let mut batch: Vec<String> = Vec::with_capacity(INSERT_BATCH_SIZE);

    match kind {
        AttachmentKind::Csv => {
            let mut file = fs::File::open(&path)?;
            let mut raw = Vec::new();
            std::io::Read::read_to_end(&mut file, &mut raw)?;
            let (text, _) = decode_csv_bytes(&raw);
            let delimiter = detect_csv_delimiter(&text);
            let mut reader = csv::ReaderBuilder::new()
                .delimiter(delimiter as u8)
                .flexible(true)
                .from_reader(text.as_bytes());
            for record in reader.records() {
                if live.cancelled.load(Ordering::SeqCst) {
                    emit_progress("cancelled", written, total_rows, "Cancelled");
                    return Ok(());
                }
                let record = record.map_err(|e| AppError::msg(format!("csv: {e}")))?;
                let values = mapped
                    .iter()
                    .map(|(idx, _)| sql_literal(record.get(*idx).unwrap_or("").trim(), &engine))
                    .collect::<Vec<_>>()
                    .join(", ");
                batch.push(format!("({values})"));
                if batch.len() >= INSERT_BATCH_SIZE {
                    flush_batch(
                        &db_engine,
                        &schema,
                        &qualified,
                        &target_cols,
                        &mut batch,
                        &mut written,
                    )
                    .await?;
                    emit_progress(
                        "load",
                        written,
                        total_rows,
                        &format!("Imported {written} rows"),
                    );
                }
            }
        }
        AttachmentKind::Xlsx | AttachmentKind::Xls => {
            use calamine::{open_workbook_auto, Reader};
            let mut workbook = open_workbook_auto(&path)
                .map_err(|e| AppError::msg(format!("open excel: {e}")))?;
            let sheet = sample
                .sheet_name
                .clone()
                .or_else(|| workbook.sheet_names().first().cloned())
                .ok_or_else(|| AppError::msg("no sheet"))?;
            let range = workbook
                .worksheet_range(&sheet)
                .map_err(|e| AppError::msg(format!("sheet: {e}")))?;
            let mut rows = range.rows();
            let _header = rows.next();
            for row in rows {
                if live.cancelled.load(Ordering::SeqCst) {
                    emit_progress("cancelled", written, total_rows, "Cancelled");
                    return Ok(());
                }
                let empty = (0..sample.headers.len()).all(|i| {
                    row.get(i)
                        .map(|c| cell_str(c).trim().is_empty())
                        .unwrap_or(true)
                });
                if empty {
                    continue;
                }
                let values = mapped
                    .iter()
                    .map(|(idx, _)| {
                        let raw = row.get(*idx).map(cell_str).unwrap_or_default();
                        sql_literal(raw.trim(), &engine)
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                batch.push(format!("({values})"));
                if batch.len() >= INSERT_BATCH_SIZE {
                    flush_batch(
                        &db_engine,
                        &schema,
                        &qualified,
                        &target_cols,
                        &mut batch,
                        &mut written,
                    )
                    .await?;
                    emit_progress(
                        "load",
                        written,
                        total_rows,
                        &format!("Imported {written} rows"),
                    );
                }
            }
        }
    }

    if !batch.is_empty() {
        if live.cancelled.load(Ordering::SeqCst) {
            emit_progress("cancelled", written, total_rows, "Cancelled");
            return Ok(());
        }
        flush_batch(
            &db_engine,
            &schema,
            &qualified,
            &target_cols,
            &mut batch,
            &mut written,
        )
        .await?;
    }

    emit_progress("done", written, total_rows, "Import completed");
    let _ = app.emit(
        IMPORT_DONE_EVENT,
        json!({
            "jobId": job_id,
            "writtenRows": written,
            "totalRows": total_rows,
            "connectionId": connection_id,
            "schema": schema,
            "table": table,
        }),
    );
    Ok(())
}

async fn flush_batch(
    engine: &LiveEngine,
    schema: &str,
    qualified: &str,
    target_cols: &str,
    batch: &mut Vec<String>,
    written: &mut u64,
) -> AppResult<()> {
    if batch.is_empty() {
        return Ok(());
    }
    let count = batch.len() as u64;
    let values = batch.join(",\n");
    batch.clear();
    let sql = format!("INSERT INTO {qualified} ({target_cols}) VALUES\n{values};");
    engine.execute_sql(Some(schema), &sql).await?;
    *written += count;
    Ok(())
}

fn sql_literal(value: &str, engine: &str) -> String {
    if value.is_empty() {
        return "NULL".into();
    }
    if matches!(value.to_ascii_lowercase().as_str(), "true" | "false")
        && engine.starts_with("postgres")
    {
        return value.to_ascii_lowercase();
    }
    if value.parse::<i128>().is_ok() || value.parse::<f64>().is_ok() {
        if !(value.len() > 1 && value.starts_with('0') && !value.contains('.')) {
            return value.to_string();
        }
    }
    let escaped = value.replace('\'', "''");
    format!("'{escaped}'")
}

fn cell_str(cell: &Data) -> String {
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

fn decode_csv_bytes(raw: &[u8]) -> (String, String) {
    if let Ok(text) = std::str::from_utf8(raw) {
        return (text.to_string(), "utf-8".into());
    }
    let (cow, _, _) = encoding_rs::GBK.decode(raw);
    (cow.into_owned(), "gbk".into())
}

fn detect_csv_delimiter(text: &str) -> char {
    let first_line = text.lines().next().unwrap_or("");
    let mut best = (',', 0usize);
    for d in [',', ';', '\t'] {
        let count = first_line.matches(d).count();
        if count > best.1 {
            best = (d, count);
        }
    }
    best.0
}
