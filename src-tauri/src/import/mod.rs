//! Tabular file attach → sample parse → scripted import (MySQL / PostgreSQL).

mod infer;
mod job;
mod parse;
mod script;

pub use infer::{build_create_table_ddl, infer_columns, SuggestedColumn};
pub use job::{
    cancel_import, import_job_status, start_import, ImportJobStatus, ImportStartRequest,
};
pub use parse::{parse_tabular_sample, AttachmentKind, ParseSample};
pub use script::{prepare_import_script, ImportPrepareRequest, ImportScriptPackage};

use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub const SAMPLE_ROW_LIMIT: usize = 20;
pub const INSERT_BATCH_SIZE: usize = 200;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentMeta {
    pub id: String,
    pub name: String,
    pub path: String,
    pub kind: AttachmentKind,
    pub size: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StageAttachmentItem {
    pub source_path: String,
    pub name: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StageAttachmentsRequest {
    pub agent_id: String,
    pub session_id: String,
    pub files: Vec<StageAttachmentItem>,
}

pub fn stage_attachments(
    chats_root: &Path,
    request: StageAttachmentsRequest,
) -> AppResult<Vec<AttachmentMeta>> {
    let agent_id = sanitize_id(&request.agent_id)?;
    let session_id = sanitize_id(&request.session_id)?;
    if request.files.is_empty() {
        return Err(AppError::msg("no files to stage"));
    }
    let dest_dir = chats_root
        .join(&agent_id)
        .join(&session_id)
        .join("attachments");
    fs::create_dir_all(&dest_dir)?;

    let mut out = Vec::with_capacity(request.files.len());
    for item in request.files {
        let source = PathBuf::from(item.source_path.trim());
        if !source.is_file() {
            return Err(AppError::msg(format!(
                "attachment not found: {}",
                source.display()
            )));
        }
        let original_name = item
            .name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| {
                source
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("attachment")
                    .to_string()
            });
        let kind = AttachmentKind::from_name(&original_name).ok_or_else(|| {
            AppError::msg(format!(
                "unsupported attachment type: {original_name} (use csv / xlsx / xls)"
            ))
        })?;
        let id = Uuid::new_v4().to_string();
        let safe_name = sanitize_filename(&original_name);
        let dest_name = format!("{id}_{safe_name}");
        let dest = dest_dir.join(&dest_name);
        fs::copy(&source, &dest)?;
        let bytes = fs::read(&dest)?;
        let size = bytes.len() as u64;
        let sha256 = hex::encode(Sha256::digest(&bytes));
        out.push(AttachmentMeta {
            id,
            name: original_name,
            path: dest.to_string_lossy().into_owned(),
            kind,
            size,
            sha256,
        });
    }
    Ok(out)
}

fn sanitize_id(value: &str) -> AppResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed.contains("..")
    {
        return Err(AppError::msg("invalid id"));
    }
    Ok(trimmed.to_string())
}

fn sanitize_filename(name: &str) -> String {
    let base = Path::new(name)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");
    let cleaned: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "file".into()
    } else {
        cleaned
    }
}

pub fn suggested_table_name(file_name: &str) -> String {
    let stem = Path::new(file_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("imported_data");
    let mut out = String::new();
    for c in stem.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        "imported_data".into()
    } else {
        trimmed.to_string()
    }
}
