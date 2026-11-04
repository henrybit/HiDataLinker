use super::infer::{build_create_table_ddl, infer_columns, SuggestedColumn};
use super::parse::{parse_tabular_sample, ParseSample};
use super::INSERT_BATCH_SIZE;
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnMapping {
    pub source: String,
    pub target: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPrepareRequest {
    pub agent_id: String,
    pub session_id: String,
    pub attachment_path: String,
    pub attachment_name: String,
    pub connection_id: String,
    pub engine: String,
    pub schema: String,
    pub table: String,
    /// `create` | `append`
    pub mode: String,
    pub create_table: bool,
    pub ddl: Option<String>,
    pub column_mappings: Option<Vec<ColumnMapping>>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportScriptPackage {
    pub job_id: String,
    pub script_dir: String,
    pub manifest_path: String,
    pub schema_sql_path: String,
    pub load_sql_path: String,
    pub ddl: String,
    pub suggested_columns: Vec<SuggestedColumn>,
    pub sample: ParseSample,
    pub total_rows: u64,
}

pub fn prepare_import_script(
    chats_root: &Path,
    request: ImportPrepareRequest,
) -> AppResult<ImportScriptPackage> {
    let agent_id = request.agent_id.trim();
    let session_id = request.session_id.trim();
    if agent_id.is_empty() || session_id.is_empty() {
        return Err(AppError::msg("agentId and sessionId are required"));
    }
    let engine = request.engine.trim().to_ascii_lowercase();
    if engine != "mysql" && engine != "postgres" && engine != "postgresql" {
        return Err(AppError::msg(
            "import currently supports mysql and postgres only",
        ));
    }
    let path = PathBuf::from(request.attachment_path.trim());
    if !path.is_file() {
        return Err(AppError::msg("attachment file not found"));
    }

    let sample = parse_tabular_sample(&path, &request.attachment_name)?;
    let suggested = infer_columns(&sample, &engine);
    let mappings = request.column_mappings.clone().unwrap_or_else(|| {
        suggested
            .iter()
            .map(|c| ColumnMapping {
                source: c.source_name.clone(),
                target: c.name.clone(),
            })
            .collect()
    });

    let ddl = if request.create_table {
        request
            .ddl
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| {
                build_create_table_ddl(&engine, &request.schema, &request.table, &suggested)
            })
    } else {
        format!(
            "-- target table already exists: {}.{}",
            request.schema, request.table
        )
    };

    let job_id = Uuid::new_v4().to_string();
    let script_dir = chats_root
        .join(agent_id)
        .join(session_id)
        .join("import-scripts")
        .join(&job_id);
    fs::create_dir_all(&script_dir)?;

    let manifest = json!({
        "jobId": job_id,
        "attachmentPath": path.to_string_lossy(),
        "attachmentName": request.attachment_name,
        "connectionId": request.connection_id,
        "engine": engine,
        "schema": request.schema,
        "table": request.table,
        "mode": request.mode,
        "createTable": request.create_table,
        "columnMappings": mappings,
        "batchSize": INSERT_BATCH_SIZE,
        "totalRows": sample.total_rows,
        "createdAt": chrono::Utc::now().to_rfc3339(),
    });
    let manifest_path = script_dir.join("manifest.json");
    fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?)?;

    let schema_sql_path = script_dir.join("schema.sql");
    fs::write(&schema_sql_path, format!("{ddl}\n"))?;

    let load_preview = format!(
        "-- Import load plan\n\
         -- Source: {}\n\
         -- Target: {}.{}.{}\n\
         -- Mode: {}\n\
         -- Total data rows: {}\n\
         -- Batch size: {}\n\
         -- Column mappings:\n{}\n\
         -- Runtime executes parameterized/batched INSERT (not raw SQL dump of all rows).\n",
        request.attachment_name,
        request.connection_id,
        request.schema,
        request.table,
        request.mode,
        sample.total_rows,
        INSERT_BATCH_SIZE,
        mappings
            .iter()
            .map(|m| format!("--   {} -> {}", m.source, m.target))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let load_sql_path = script_dir.join("load.sql");
    fs::write(&load_sql_path, load_preview)?;

    // Persist sample + columns for the executor.
    let plan_path = script_dir.join("plan.json");
    let plan = json!({
        "ddl": ddl,
        "createTable": request.create_table,
        "suggestedColumns": suggested,
        "sampleHeaders": sample.headers,
    });
    fs::write(&plan_path, serde_json::to_vec_pretty(&plan)?)?;

    Ok(ImportScriptPackage {
        job_id,
        script_dir: script_dir.to_string_lossy().into_owned(),
        manifest_path: manifest_path.to_string_lossy().into_owned(),
        schema_sql_path: schema_sql_path.to_string_lossy().into_owned(),
        load_sql_path: load_sql_path.to_string_lossy().into_owned(),
        ddl,
        suggested_columns: suggested,
        total_rows: sample.total_rows,
        sample,
    })
}
