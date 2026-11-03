use crate::db::DatabaseEngine;
use crate::error::{AppError, AppResult};
use crate::models::ConnectionProfile;
use crate::runtime::run_blocking;
use crate::state::AppState;
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

const REFRESH_AFTER_HOURS: i64 = 24;
const CLEAR_AFTER_FAIL_DAYS: i64 = 7;
const MAX_SCHEMAS_PER_CONN: usize = 24;
const MAX_TABLES_PER_SCHEMA: usize = 200;
const MAX_DIGEST_SCHEMAS: usize = 12;
const MAX_DIGEST_TABLES: usize = 40;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SchemaCacheStatus {
    Ok,
    Error,
}

impl Default for SchemaCacheStatus {
    fn default() -> Self {
        Self::Ok
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionSchemaMeta {
    /// Last successful refresh time (ISO-8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub status: SchemaCacheStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_attempt_at: Option<String>,
    /// Start of the current consecutive failure streak.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fail_since: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SchemaIndex {
    #[serde(default)]
    pub connections: HashMap<String, ConnectionSchemaMeta>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaDatabaseSnapshot {
    pub name: String,
    #[serde(default)]
    pub tables: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionSchemaSnapshot {
    pub connection_id: String,
    pub name: String,
    pub engine: String,
    pub refreshed_at: String,
    #[serde(default)]
    pub databases: Vec<SchemaDatabaseSnapshot>,
}

fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn parse_iso(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

fn validate_connection_id(id: &str) -> AppResult<()> {
    if id.is_empty()
        || id.contains('/')
        || id.contains('\\')
        || id.contains("..")
        || id.contains('\0')
    {
        return Err(AppError::msg("invalid connection id"));
    }
    Ok(())
}

fn schema_dir(root: &Path, connection_id: &str) -> AppResult<PathBuf> {
    validate_connection_id(connection_id)?;
    Ok(root.join(connection_id))
}

fn snapshot_path(root: &Path, connection_id: &str) -> AppResult<PathBuf> {
    Ok(schema_dir(root, connection_id)?.join("snapshot.json"))
}

pub fn load_index(path: &Path) -> AppResult<SchemaIndex> {
    if !path.exists() {
        return Ok(SchemaIndex::default());
    }
    let raw = fs::read_to_string(path)?;
    if raw.trim().is_empty() {
        return Ok(SchemaIndex::default());
    }
    Ok(serde_json::from_str(&raw)?)
}

pub fn save_index(path: &Path, index: &SchemaIndex) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(index)?;
    fs::write(path, json)?;
    Ok(())
}

pub fn load_snapshot(root: &Path, connection_id: &str) -> AppResult<Option<ConnectionSchemaSnapshot>> {
    let path = snapshot_path(root, connection_id)?;
    if !path.is_file() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path)?;
    Ok(Some(serde_json::from_str(&raw)?))
}

pub fn clear_connection_schema(root: &Path, connection_id: &str) -> AppResult<()> {
    let dir = schema_dir(root, connection_id)?;
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    Ok(())
}

pub fn needs_refresh(meta: Option<&ConnectionSchemaMeta>, now: DateTime<Utc>) -> bool {
    let Some(meta) = meta else {
        return true;
    };
    let freshness_anchor = meta
        .updated_at
        .as_deref()
        .and_then(parse_iso)
        .or_else(|| meta.last_attempt_at.as_deref().and_then(parse_iso));
    match freshness_anchor {
        None => true,
        Some(at) => now - at >= Duration::hours(REFRESH_AFTER_HOURS),
    }
}

pub fn should_clear_after_failures(meta: &ConnectionSchemaMeta, now: DateTime<Utc>) -> bool {
    if meta.status != SchemaCacheStatus::Error {
        return false;
    }
    let Some(since) = meta.fail_since.as_deref().and_then(parse_iso) else {
        return false;
    };
    now - since >= Duration::days(CLEAR_AFTER_FAIL_DAYS)
}

fn write_snapshot(root: &Path, snapshot: &ConnectionSchemaSnapshot) -> AppResult<()> {
    let dir = schema_dir(root, &snapshot.connection_id)?;
    fs::create_dir_all(&dir)?;
    let path = dir.join("snapshot.json");
    fs::write(path, serde_json::to_string_pretty(snapshot)?)?;
    Ok(())
}

/// Drop schema files for connections that failed continuously for 7+ days.
pub fn prune_stale_failures(index_path: &Path, schemas_root: &Path) -> AppResult<SchemaIndex> {
    let mut index = load_index(index_path)?;
    let now = Utc::now();
    let mut changed = false;
    for (id, meta) in index.connections.iter_mut() {
        if should_clear_after_failures(meta, now) {
            let _ = clear_connection_schema(schemas_root, id);
            meta.updated_at = None;
            changed = true;
            log::info!("schema cache cleared for connection {id} after 7 days of failures");
        }
    }
    if changed {
        save_index(index_path, &index)?;
    }
    Ok(index)
}

async fn refresh_one(state: &AppState, profile: &ConnectionProfile) -> AppResult<()> {
    let index_path = state.schema_index_path().clone();
    let schemas_root = state.schema_cache_dir().clone();
    let connection_id = profile.id.clone();
    let name = profile.name.clone();
    let engine_name = profile.engine.clone();

    let attempt_at = now_iso();
    let outcome = match refresh_connection_live(state, profile).await {
        Ok(databases) => Ok(databases),
        Err(error) => Err(error.user_message()),
    };

    let _guard = state.schema_refresh_lock().lock().await;
    let schemas_root_for_write = schemas_root.clone();
    let connection_id_for_write = connection_id.clone();
    run_blocking(move || {
        let mut index = load_index(&index_path)?;
        let entry = index
            .connections
            .entry(connection_id_for_write.clone())
            .or_default();
        entry.last_attempt_at = Some(attempt_at.clone());

        match outcome {
            Ok(databases) => {
                let refreshed_at = attempt_at.clone();
                let snapshot = ConnectionSchemaSnapshot {
                    connection_id: connection_id_for_write.clone(),
                    name,
                    engine: engine_name,
                    refreshed_at: refreshed_at.clone(),
                    databases,
                };
                write_snapshot(&schemas_root_for_write, &snapshot)?;
                entry.status = SchemaCacheStatus::Ok;
                entry.updated_at = Some(refreshed_at);
                entry.last_error = None;
                entry.fail_since = None;
            }
            Err(message) => {
                entry.status = SchemaCacheStatus::Error;
                entry.last_error = Some(message);
                if entry.fail_since.is_none() {
                    entry.fail_since = Some(attempt_at.clone());
                }
                if should_clear_after_failures(entry, Utc::now()) {
                    let _ = clear_connection_schema(&schemas_root_for_write, &connection_id_for_write);
                    entry.updated_at = None;
                }
            }
        }
        save_index(&index_path, &index)?;
        Ok(())
    })
    .await?;
    Ok(())
}

async fn refresh_connection_live(
    state: &AppState,
    profile: &ConnectionProfile,
) -> AppResult<Vec<SchemaDatabaseSnapshot>> {
    if profile.password.as_deref().unwrap_or("").is_empty() {
        return Err(AppError::msg(
            "this connection has no saved password; connect it once in HiDataLinker before schema cache can refresh",
        ));
    }
    if !state.is_connected(&profile.id) {
        state.connect(&profile.id, None).await?;
    }
    let engine = state.engine(&profile.id)?;
    let databases = crate::runtime::run_db(async move { engine.list_databases().await }).await?;
    let mut out = Vec::new();
    for db in databases.into_iter().take(MAX_SCHEMAS_PER_CONN) {
        let schema = db.name.trim().to_string();
        if schema.is_empty() {
            continue;
        }
        let engine = state.engine(&profile.id)?;
        let schema_for_list = schema.clone();
        let tables = match crate::runtime::run_db(async move {
            engine.list_tables(&schema_for_list).await
        })
        .await
        {
            Ok(tables) => tables
                .into_iter()
                .map(|table| table.name)
                .filter(|name| !name.is_empty())
                .take(MAX_TABLES_PER_SCHEMA)
                .collect(),
            Err(error) => {
                log::warn!(
                    "schema cache list_tables failed for {} / {}: {}",
                    profile.id,
                    schema,
                    error.user_message()
                );
                Vec::new()
            }
        };
        out.push(SchemaDatabaseSnapshot {
            name: schema,
            tables,
        });
    }
    Ok(out)
}

/// Refresh every connection whose cache is older than 24h (or missing).
pub async fn refresh_stale(state: &AppState) -> AppResult<()> {
    let index_path = state.schema_index_path().clone();
    let schemas_root = state.schema_cache_dir().clone();
    let _ = run_blocking(move || prune_stale_failures(&index_path, &schemas_root)).await;

    let profiles = state.list_profiles().await;
    let index = load_index(state.schema_index_path())?;
    let now = Utc::now();
    for profile in profiles {
        let meta = index.connections.get(&profile.id);
        if !needs_refresh(meta, now) {
            continue;
        }
        if let Err(error) = refresh_one(state, &profile).await {
            log::warn!(
                "schema cache refresh failed for {}: {}",
                profile.id,
                error.user_message()
            );
        }
    }

    // Drop index rows for deleted connections and wipe orphan schema dirs.
    let profiles = state.list_profiles().await;
    let live_ids: std::collections::HashSet<String> =
        profiles.into_iter().map(|p| p.id).collect();
    let index_path = state.schema_index_path().clone();
    let schemas_root = state.schema_cache_dir().clone();
    let _guard = state.schema_refresh_lock().lock().await;
    run_blocking(move || {
        let mut index = load_index(&index_path)?;
        let stale_ids: Vec<String> = index
            .connections
            .keys()
            .filter(|id| !live_ids.contains(*id))
            .cloned()
            .collect();
        for id in &stale_ids {
            index.connections.remove(id);
            let _ = clear_connection_schema(&schemas_root, id);
        }
        if !stale_ids.is_empty() {
            save_index(&index_path, &index)?;
        }
        if schemas_root.is_dir() {
            for entry in fs::read_dir(&schemas_root)? {
                let entry = entry?;
                if !entry.file_type()?.is_dir() {
                    continue;
                }
                let name = entry.file_name();
                let Some(id) = name.to_str() else {
                    continue;
                };
                if !live_ids.contains(id) {
                    let _ = fs::remove_dir_all(entry.path());
                }
            }
        }
        Ok(())
    })
    .await?;
    Ok(())
}

pub fn build_digest_from_snapshots(
    schemas_root: &Path,
    index: &SchemaIndex,
    profiles: &[ConnectionProfile],
) -> String {
    let mut lines = Vec::new();
    for profile in profiles.iter().take(12) {
        let meta = index.connections.get(&profile.id);
        let status = meta
            .map(|m| match m.status {
                SchemaCacheStatus::Ok => "ok",
                SchemaCacheStatus::Error => "error",
            })
            .unwrap_or("missing");
        let updated = meta
            .and_then(|m| m.updated_at.as_deref())
            .unwrap_or("-");
        let err = meta
            .and_then(|m| m.last_error.as_deref())
            .unwrap_or("");
        lines.push(format!(
            "- {} ({}) id={} cache={} updatedAt={}",
            profile.name, profile.engine, profile.id, status, updated
        ));
        if status == "error" && !err.is_empty() {
            lines.push(format!("  error: {err}"));
        }
        match load_snapshot(schemas_root, &profile.id) {
            Ok(Some(snapshot)) => {
                for db in snapshot.databases.iter().take(MAX_DIGEST_SCHEMAS) {
                    let names: Vec<&str> = db
                        .tables
                        .iter()
                        .take(MAX_DIGEST_TABLES)
                        .map(String::as_str)
                        .collect();
                    lines.push(format!(
                        "  schema {}: {}",
                        db.name,
                        if names.is_empty() {
                            "(no tables)".to_string()
                        } else {
                            names.join(", ")
                        }
                    ));
                }
            }
            Ok(None) => {
                if status != "error" {
                    lines.push("  schema: (not cached yet)".into());
                }
            }
            Err(error) => {
                lines.push(format!("  schema: (cache read failed: {})", error.user_message()));
            }
        }
    }
    if lines.is_empty() {
        "(no connections)".into()
    } else {
        lines.join("\n")
    }
}

fn filter_profiles(
    profiles: Vec<ConnectionProfile>,
    connection_ids: Option<&[String]>,
) -> Vec<ConnectionProfile> {
    let Some(ids) = connection_ids else {
        return profiles;
    };
    if ids.is_empty() {
        return profiles;
    }
    let set: std::collections::HashSet<&str> = ids.iter().map(String::as_str).collect();
    profiles
        .into_iter()
        .filter(|profile| set.contains(profile.id.as_str()))
        .collect()
}

/// Connect saved-password profiles that are not yet open (best-effort, capped by session limit).
pub async fn warm_connections(
    state: &AppState,
    connection_ids: Option<&[String]>,
) -> AppResult<()> {
    let profiles = filter_profiles(state.list_profiles().await, connection_ids);
    for profile in profiles {
        if state.is_connected(&profile.id) {
            continue;
        }
        if profile.password.as_deref().unwrap_or("").is_empty() {
            continue;
        }
        match state.connect(&profile.id, None).await {
            Ok(_) => log::info!("warmed agent connection {}", profile.id),
            Err(error) => log::warn!(
                "warm connection {} failed: {}",
                profile.id,
                error.user_message()
            ),
        }
    }
    Ok(())
}

/// Ensure stale caches are refreshed, then return a digest string for the analyst agent.
/// When `connection_ids` is non-empty, only those connections are warmed into the digest.
pub async fn ensure_and_digest(
    state: &AppState,
    connection_ids: Option<&[String]>,
) -> AppResult<String> {
    // Open live sessions first so tools during the run do not hit NotConnected,
    // and so a stale cache refresh can succeed.
    let _ = warm_connections(state, connection_ids).await;
    refresh_stale(state).await?;
    let profiles = filter_profiles(state.list_profiles().await, connection_ids);
    let index_path = state.schema_index_path().clone();
    let schemas_root = state.schema_cache_dir().clone();
    run_blocking(move || {
        let index = load_index(&index_path)?;
        Ok(build_digest_from_snapshots(
            &schemas_root,
            &index,
            &profiles,
        ))
    })
    .await
}

pub async fn remove_connection(state: &AppState, connection_id: &str) -> AppResult<()> {
    let index_path = state.schema_index_path().clone();
    let schemas_root = state.schema_cache_dir().clone();
    let connection_id = connection_id.to_string();
    let _guard = state.schema_refresh_lock().lock().await;
    run_blocking(move || {
        let mut index = load_index(&index_path)?;
        index.connections.remove(&connection_id);
        save_index(&index_path, &index)?;
        clear_connection_schema(&schemas_root, &connection_id)?;
        Ok(())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_needed_when_missing_or_stale() {
        let now = Utc::now();
        assert!(needs_refresh(None, now));
        let fresh = ConnectionSchemaMeta {
            updated_at: Some(now.to_rfc3339_opts(SecondsFormat::Millis, true)),
            status: SchemaCacheStatus::Ok,
            ..Default::default()
        };
        assert!(!needs_refresh(Some(&fresh), now));
        let stale = ConnectionSchemaMeta {
            updated_at: Some(
                (now - Duration::hours(25)).to_rfc3339_opts(SecondsFormat::Millis, true),
            ),
            status: SchemaCacheStatus::Ok,
            ..Default::default()
        };
        assert!(needs_refresh(Some(&stale), now));
    }

    #[test]
    fn clears_after_seven_day_failure_streak() {
        let now = Utc::now();
        let meta = ConnectionSchemaMeta {
            status: SchemaCacheStatus::Error,
            fail_since: Some(
                (now - Duration::days(7) - Duration::hours(1))
                    .to_rfc3339_opts(SecondsFormat::Millis, true),
            ),
            ..Default::default()
        };
        assert!(should_clear_after_failures(&meta, now));
        let recent = ConnectionSchemaMeta {
            status: SchemaCacheStatus::Error,
            fail_since: Some(
                (now - Duration::days(2)).to_rfc3339_opts(SecondsFormat::Millis, true),
            ),
            ..Default::default()
        };
        assert!(!should_clear_after_failures(&recent, now));
    }
}
