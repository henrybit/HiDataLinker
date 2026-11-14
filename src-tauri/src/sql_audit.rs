//! Persistent audit trail of every SQL statement the client sends to a user database.
//!
//! Entries are appended as JSON lines to `~/.hidatalinker/audit/YYYY-MM-DD.jsonl`.

use crate::error::{AppError, AppResult};
use crate::models::ConnectionProfile;
use chrono::{Duration, Local, NaiveDate};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

pub const DEFAULT_RETAIN_DAYS: u32 = 90;
pub const MAX_RETAIN_DAYS: u32 = 3650;
/// Larger statements (bulk import batches) keep their head so a single run cannot balloon the log.
pub const MAX_SQL_BYTES: usize = 256 * 1024;
const DEFAULT_PAGE: usize = 200;
const MAX_PAGE: usize = 2000;
const MAX_ERROR_CHARS: usize = 4000;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SqlAuditSettings {
    pub enabled: bool,
    pub retain_days: u32,
}

impl Default for SqlAuditSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            retain_days: DEFAULT_RETAIN_DAYS,
        }
    }
}

/// Where a statement originated inside the client.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditSource {
    Editor,
    TableBrowse,
    TableEdit,
    Structure,
    Analysis,
    Schema,
    Export,
    Migration,
    Import,
    Agent,
    /// Catalog reads made by the in-workspace SQL drafting assistant.
    Assist,
    App,
}

impl AuditSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Editor => "editor",
            Self::TableBrowse => "table_browse",
            Self::TableEdit => "table_edit",
            Self::Structure => "structure",
            Self::Analysis => "analysis",
            Self::Schema => "schema",
            Self::Export => "export",
            Self::Migration => "migration",
            Self::Import => "import",
            Self::Agent => "agent",
            Self::Assist => "assist",
            Self::App => "app",
        }
    }

    /// Sources the webview may claim for `execute_sql`; anything else is treated as the editor.
    pub fn from_client(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some("table_edit") => Self::TableEdit,
            Some("structure") => Self::Structure,
            Some("analysis") => Self::Analysis,
            Some("assist") => Self::Assist,
            _ => Self::Editor,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SqlAuditEntry {
    pub id: String,
    pub at: String,
    pub os_user: String,
    pub connection_id: String,
    pub connection_name: String,
    pub engine: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub schema: Option<String>,
    pub source: String,
    pub source_detail: Option<String>,
    pub operation: String,
    pub sql: String,
    pub sql_length: usize,
    pub sql_truncated: bool,
    pub statement_kind: Option<String>,
    pub success: bool,
    pub affected_rows: Option<u64>,
    pub row_count: Option<u64>,
    pub duration_ms: u64,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SqlAuditQuery {
    /// `YYYY-MM-DD`; defaults to today.
    pub date: Option<String>,
    pub connection_id: Option<String>,
    pub source: Option<String>,
    /// `ok` | `error`; anything else returns both.
    pub status: Option<String>,
    pub keyword: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SqlAuditPage {
    pub date: String,
    pub total: usize,
    pub entries: Vec<SqlAuditEntry>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SqlAuditDay {
    pub date: String,
    pub size_bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SqlAuditInfo {
    pub settings: SqlAuditSettings,
    pub dir: String,
}

struct Inner {
    dir: PathBuf,
    settings_path: PathBuf,
    enabled: AtomicBool,
    retain_days: AtomicU32,
    write_lock: Mutex<()>,
    os_user: String,
}

#[derive(Clone)]
pub struct SqlAuditLog {
    inner: Arc<Inner>,
}

impl SqlAuditLog {
    pub fn open(dir: PathBuf, settings_path: PathBuf) -> AppResult<Self> {
        fs::create_dir_all(&dir)?;
        let settings = load_settings(&settings_path).unwrap_or_default();
        let log = Self {
            inner: Arc::new(Inner {
                dir,
                settings_path,
                enabled: AtomicBool::new(settings.enabled),
                retain_days: AtomicU32::new(clamp_retain(settings.retain_days)),
                write_lock: Mutex::new(()),
                os_user: os_user(),
            }),
        };
        if let Err(error) = log.cleanup() {
            log::warn!("sql audit cleanup failed: {}", error.user_message());
        }
        Ok(log)
    }

    pub fn settings(&self) -> SqlAuditSettings {
        SqlAuditSettings {
            enabled: self.inner.enabled.load(Ordering::Relaxed),
            retain_days: self.inner.retain_days.load(Ordering::Relaxed),
        }
    }

    pub fn info(&self) -> SqlAuditInfo {
        SqlAuditInfo {
            settings: self.settings(),
            dir: self.inner.dir.display().to_string(),
        }
    }

    pub fn update_settings(&self, settings: SqlAuditSettings) -> AppResult<SqlAuditSettings> {
        let settings = SqlAuditSettings {
            enabled: settings.enabled,
            retain_days: clamp_retain(settings.retain_days),
        };
        save_settings(&self.inner.settings_path, &settings)?;
        self.inner
            .enabled
            .store(settings.enabled, Ordering::Relaxed);
        self.inner
            .retain_days
            .store(settings.retain_days, Ordering::Relaxed);
        self.cleanup()?;
        Ok(settings)
    }

    pub fn is_enabled(&self) -> bool {
        self.inner.enabled.load(Ordering::Relaxed)
    }

    pub fn os_user(&self) -> &str {
        &self.inner.os_user
    }

    /// Append one entry. Failures are logged, never surfaced to the statement's caller.
    pub fn append(&self, entry: &SqlAuditEntry) {
        if !self.is_enabled() {
            return;
        }
        if let Err(error) = self.write_line(entry) {
            log::warn!("sql audit write failed: {}", error.user_message());
        }
    }

    fn write_line(&self, entry: &SqlAuditEntry) -> AppResult<()> {
        let mut line = serde_json::to_string(entry)?;
        line.push('\n');
        let path = self.day_path(&Local::now().format("%Y-%m-%d").to_string());
        let _guard = self
            .inner
            .write_lock
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        file.write_all(line.as_bytes())?;
        Ok(())
    }

    pub fn list_days(&self) -> AppResult<Vec<SqlAuditDay>> {
        let mut days = Vec::new();
        for entry in fs::read_dir(&self.inner.dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let name = entry.file_name();
            let Some(date) = name.to_str().and_then(parse_day_name) else {
                continue;
            };
            days.push(SqlAuditDay {
                date: date.format("%Y-%m-%d").to_string(),
                size_bytes: entry.metadata()?.len(),
            });
        }
        days.sort_by(|a, b| b.date.cmp(&a.date));
        Ok(days)
    }

    /// Entries for one day, newest first, after filters.
    pub fn query(&self, query: &SqlAuditQuery) -> AppResult<SqlAuditPage> {
        let date = match query.date.as_deref().map(str::trim) {
            Some(value) if !value.is_empty() => NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .map_err(|_| AppError::msg("invalid audit date, expected YYYY-MM-DD"))?,
            _ => Local::now().date_naive(),
        };
        let date = date.format("%Y-%m-%d").to_string();
        let path = self.day_path(&date);
        let mut matched = Vec::new();
        if path.is_file() {
            let filter = Filter::new(query);
            let reader = BufReader::new(fs::File::open(&path)?);
            for line in reader.lines() {
                let line = line?;
                if line.trim().is_empty() {
                    continue;
                }
                let Ok(entry) = serde_json::from_str::<SqlAuditEntry>(&line) else {
                    continue;
                };
                if filter.matches(&entry) {
                    matched.push(entry);
                }
            }
        }
        matched.reverse();
        let total = matched.len();
        let limit = query.limit.unwrap_or(DEFAULT_PAGE).clamp(1, MAX_PAGE);
        let offset = query.offset.unwrap_or(0);
        let entries = matched.into_iter().skip(offset).take(limit).collect();
        Ok(SqlAuditPage {
            date,
            total,
            entries,
        })
    }

    fn day_path(&self, date: &str) -> PathBuf {
        self.inner.dir.join(format!("{date}.jsonl"))
    }

    fn cleanup(&self) -> AppResult<usize> {
        cleanup_old_days(
            &self.inner.dir,
            self.inner.retain_days.load(Ordering::Relaxed),
        )
    }
}

struct Filter {
    connection_id: Option<String>,
    source: Option<String>,
    status: Option<bool>,
    keyword: Option<String>,
}

impl Filter {
    fn new(query: &SqlAuditQuery) -> Self {
        let non_empty = |value: &Option<String>| {
            value
                .as_deref()
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_string)
        };
        Self {
            connection_id: non_empty(&query.connection_id),
            source: non_empty(&query.source),
            status: match query.status.as_deref() {
                Some("ok") => Some(true),
                Some("error") => Some(false),
                _ => None,
            },
            keyword: non_empty(&query.keyword).map(|item| item.to_lowercase()),
        }
    }

    fn matches(&self, entry: &SqlAuditEntry) -> bool {
        if let Some(id) = &self.connection_id {
            if &entry.connection_id != id {
                return false;
            }
        }
        if let Some(source) = &self.source {
            if &entry.source != source {
                return false;
            }
        }
        if let Some(success) = self.status {
            if entry.success != success {
                return false;
            }
        }
        if let Some(keyword) = &self.keyword {
            let hit = entry.sql.to_lowercase().contains(keyword)
                || entry.connection_name.to_lowercase().contains(keyword)
                || entry
                    .schema
                    .as_deref()
                    .is_some_and(|schema| schema.to_lowercase().contains(keyword))
                || entry
                    .error
                    .as_deref()
                    .is_some_and(|error| error.to_lowercase().contains(keyword));
            if !hit {
                return false;
            }
        }
        true
    }
}

/// Connection facts captured once per session so each entry is self-describing.
#[derive(Clone, Debug)]
pub struct AuditConnection {
    pub id: String,
    pub name: String,
    pub engine: String,
    pub host: String,
    pub port: u16,
    pub username: String,
}

impl From<&ConnectionProfile> for AuditConnection {
    fn from(profile: &ConnectionProfile) -> Self {
        Self {
            id: profile.id.clone(),
            name: profile.name.clone(),
            engine: profile.engine.clone(),
            host: profile.host.clone(),
            port: profile.port,
            username: profile.username.clone(),
        }
    }
}

/// Outcome details known after a statement finishes successfully.
#[derive(Clone, Debug, Default)]
pub struct AuditOutcome {
    pub statement_kind: Option<String>,
    pub affected_rows: Option<u64>,
    pub row_count: Option<u64>,
}

/// Audit binding carried by a live engine handle: the session plus the caller that issued SQL.
#[derive(Clone)]
pub struct AuditScope {
    log: SqlAuditLog,
    connection: Arc<AuditConnection>,
    source: AuditSource,
    source_detail: Option<String>,
}

impl AuditScope {
    pub fn new(log: SqlAuditLog, connection: AuditConnection) -> Self {
        Self {
            log,
            connection: Arc::new(connection),
            source: AuditSource::App,
            source_detail: None,
        }
    }

    pub fn with_source(mut self, source: AuditSource, detail: Option<String>) -> Self {
        self.source = source;
        self.source_detail = detail.filter(|item| !item.trim().is_empty());
        self
    }

    pub fn record(
        &self,
        operation: &str,
        schema: Option<&str>,
        sql: &str,
        duration_ms: u64,
        result: Result<AuditOutcome, String>,
    ) {
        if !self.log.is_enabled() {
            return;
        }
        let masked = mask_secrets(sql);
        let sql_length = masked.len();
        let (sql, sql_truncated) = truncate_bytes(&masked, MAX_SQL_BYTES);
        let (success, outcome, error) = match result {
            Ok(outcome) => (true, outcome, None),
            Err(error) => (
                false,
                AuditOutcome::default(),
                Some(error.chars().take(MAX_ERROR_CHARS).collect()),
            ),
        };
        let connection = &self.connection;
        let entry = SqlAuditEntry {
            id: uuid::Uuid::new_v4().to_string(),
            at: Local::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, false),
            os_user: self.log.os_user().to_string(),
            connection_id: connection.id.clone(),
            connection_name: connection.name.clone(),
            engine: connection.engine.clone(),
            host: connection.host.clone(),
            port: connection.port,
            username: connection.username.clone(),
            schema: schema
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_string),
            source: self.source.as_str().to_string(),
            source_detail: self.source_detail.clone(),
            operation: operation.to_string(),
            sql: sql.to_string(),
            sql_length,
            sql_truncated,
            statement_kind: outcome.statement_kind,
            success,
            affected_rows: outcome.affected_rows,
            row_count: outcome.row_count,
            duration_ms,
            error,
        };
        self.log.append(&entry);
    }
}

fn clamp_retain(days: u32) -> u32 {
    days.clamp(1, MAX_RETAIN_DAYS)
}

fn os_user() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default()
}

fn load_settings(path: &Path) -> AppResult<SqlAuditSettings> {
    if !path.exists() {
        return Ok(SqlAuditSettings::default());
    }
    let raw = fs::read_to_string(path)?;
    if raw.trim().is_empty() {
        return Ok(SqlAuditSettings::default());
    }
    Ok(serde_json::from_str(&raw)?)
}

fn save_settings(path: &Path, settings: &SqlAuditSettings) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(settings)?)?;
    Ok(())
}

fn parse_day_name(name: &str) -> Option<NaiveDate> {
    let stem = name.strip_suffix(".jsonl")?;
    NaiveDate::parse_from_str(stem, "%Y-%m-%d").ok()
}

fn cleanup_old_days(dir: &Path, retain_days: u32) -> AppResult<usize> {
    if !dir.is_dir() {
        return Ok(0);
    }
    let cutoff = Local::now().date_naive() - Duration::days(i64::from(retain_days));
    let mut removed = 0;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name();
        let Some(date) = name.to_str().and_then(parse_day_name) else {
            continue;
        };
        if date < cutoff {
            fs::remove_file(entry.path())?;
            removed += 1;
        }
    }
    Ok(removed)
}

fn truncate_bytes(text: &str, max: usize) -> (&str, bool) {
    if text.len() <= max {
        return (text, false);
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (&text[..end], true)
}

/// Replace the quoted literal after `IDENTIFIED BY` / `PASSWORD` with `'******'`.
fn mask_secrets(sql: &str) -> String {
    const MARKERS: [&str; 2] = ["identified by", "password"];
    let lower = sql.to_ascii_lowercase();
    let bytes = sql.as_bytes();
    let mut out = String::with_capacity(sql.len());
    let mut cursor = 0;
    while cursor < sql.len() {
        let next = MARKERS
            .iter()
            .filter_map(|marker| {
                lower[cursor..]
                    .find(marker)
                    .map(|at| (cursor + at, marker.len()))
            })
            .min_by_key(|(at, _)| *at);
        let Some((at, len)) = next else {
            break;
        };
        let mut scan = at + len;
        while scan < bytes.len() && (bytes[scan].is_ascii_whitespace() || bytes[scan] == b'=') {
            scan += 1;
        }
        let quote = bytes.get(scan).copied();
        if !matches!(quote, Some(b'\'') | Some(b'"')) {
            out.push_str(&sql[cursor..at + len]);
            cursor = at + len;
            continue;
        }
        let quote = quote.unwrap();
        let mut end = scan + 1;
        while end < bytes.len() {
            if bytes[end] == quote {
                if bytes.get(end + 1) == Some(&quote) {
                    end += 2;
                    continue;
                }
                break;
            }
            end += 1;
        }
        out.push_str(&sql[cursor..scan]);
        out.push(quote as char);
        out.push_str("******");
        out.push(quote as char);
        cursor = (end + 1).min(sql.len());
    }
    out.push_str(&sql[cursor..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_log() -> (SqlAuditLog, PathBuf) {
        let root = std::env::temp_dir().join(format!("hdl-audit-{}", uuid::Uuid::new_v4()));
        let log = SqlAuditLog::open(root.join("audit"), root.join("settings").join("audit.json"))
            .unwrap();
        (log, root)
    }

    fn scope(log: &SqlAuditLog) -> AuditScope {
        AuditScope::new(
            log.clone(),
            AuditConnection {
                id: "c1".into(),
                name: "local mysql".into(),
                engine: "mysql".into(),
                host: "127.0.0.1".into(),
                port: 3306,
                username: "root".into(),
            },
        )
    }

    #[test]
    fn records_and_queries_newest_first() {
        let (log, root) = temp_log();
        let editor = scope(&log).with_source(AuditSource::Editor, None);
        editor.record(
            "execute_sql",
            Some("shop"),
            "select 1",
            3,
            Ok(AuditOutcome {
                statement_kind: Some("select".into()),
                affected_rows: Some(0),
                row_count: Some(1),
            }),
        );
        editor.record(
            "execute_sql",
            Some("shop"),
            "delete from orders",
            5,
            Err("permission denied".into()),
        );

        let page = log.query(&SqlAuditQuery::default()).unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.entries[0].sql, "delete from orders");
        assert!(!page.entries[0].success);
        assert_eq!(page.entries[1].row_count, Some(1));
        assert_eq!(page.entries[1].source, "editor");
        assert_eq!(page.entries[1].connection_name, "local mysql");

        let errors = log
            .query(&SqlAuditQuery {
                status: Some("error".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(errors.total, 1);

        let keyword = log
            .query(&SqlAuditQuery {
                keyword: Some("SELECT".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(keyword.total, 1);
        assert_eq!(log.list_days().unwrap().len(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn disabled_log_writes_nothing() {
        let (log, root) = temp_log();
        log.update_settings(SqlAuditSettings {
            enabled: false,
            retain_days: 30,
        })
        .unwrap();
        scope(&log).record(
            "execute_sql",
            None,
            "select 1",
            1,
            Ok(AuditOutcome::default()),
        );
        assert_eq!(log.query(&SqlAuditQuery::default()).unwrap().total, 0);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn cleanup_drops_days_past_retention() {
        let (log, root) = temp_log();
        let dir = root.join("audit");
        let today = Local::now().date_naive();
        let keep = (today - Duration::days(2)).format("%Y-%m-%d").to_string();
        let drop = (today - Duration::days(40)).format("%Y-%m-%d").to_string();
        fs::write(dir.join(format!("{keep}.jsonl")), "").unwrap();
        fs::write(dir.join(format!("{drop}.jsonl")), "").unwrap();
        log.update_settings(SqlAuditSettings {
            enabled: true,
            retain_days: 30,
        })
        .unwrap();
        assert!(dir.join(format!("{keep}.jsonl")).is_file());
        assert!(!dir.join(format!("{drop}.jsonl")).exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn truncates_large_sql_on_char_boundary() {
        let text = "é".repeat(10);
        let (head, truncated) = truncate_bytes(&text, 5);
        assert!(truncated);
        assert_eq!(head, "éé");
    }

    #[test]
    fn masks_password_literals() {
        assert_eq!(
            mask_secrets("CREATE USER 'bob'@'%' IDENTIFIED BY 's3cr''et'"),
            "CREATE USER 'bob'@'%' IDENTIFIED BY '******'"
        );
        assert_eq!(
            mask_secrets("ALTER ROLE app WITH PASSWORD 'p@ss' LOGIN"),
            "ALTER ROLE app WITH PASSWORD '******' LOGIN"
        );
        assert_eq!(
            mask_secrets("select password from users"),
            "select password from users"
        );
    }

    #[test]
    fn client_source_whitelist() {
        assert_eq!(
            AuditSource::from_client(Some("table_edit")),
            AuditSource::TableEdit
        );
        assert_eq!(AuditSource::from_client(Some("agent")), AuditSource::Editor);
        assert_eq!(AuditSource::from_client(Some("assist")), AuditSource::Assist);
        assert_eq!(AuditSource::from_client(None), AuditSource::Editor);
    }
}
