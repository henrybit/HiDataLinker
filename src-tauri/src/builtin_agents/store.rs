use crate::error::{AppError, AppResult};
use chrono::SecondsFormat;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

const DATA_ANALYST_ID: &str = "data-analyst";
const MAX_SESSIONS: usize = 80;
const MAX_TITLE: usize = 200;
const MAX_ENTRY_BYTES: usize = 512 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub kind: String,
    pub description: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionMeta {
    pub id: String,
    pub agent_id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub status: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewAgentSession {
    pub title: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptEntry {
    pub id: String,
    pub at: String,
    pub role: String,
    pub kind: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionDetail {
    pub meta: AgentSessionMeta,
    pub transcript: Vec<TranscriptEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<Value>,
}

pub fn ensure_builtins(agents_root: &Path) -> AppResult<()> {
    let dir = agents_root.join(DATA_ANALYST_ID);
    fs::create_dir_all(dir.join("sessions"))?;
    let agent_json = dir.join("agent.json");
    if !agent_json.exists() {
        let info = AgentInfo {
            id: DATA_ANALYST_ID.into(),
            name: "Data Analyst".into(),
            version: "1.0.0".into(),
            kind: "builtin".into(),
            description: "Multi-database read-only analysis agent powered by LangGraph".into(),
        };
        fs::write(agent_json, serde_json::to_vec_pretty(&info)?)?;
    }
    let system = dir.join("system.md");
    if !system.exists() {
        fs::write(system, DEFAULT_SYSTEM_MD)?;
    }
    Ok(())
}

const DEFAULT_SYSTEM_MD: &str = r#"# Data Analyst Agent

You are HiDataLinker's built-in data analysis agent. You help users answer questions about data across their connected databases.

## Rules

- Use only read-only tools: list_connections, connect, list_databases, list_tables, list_views, get_columns, get_ddl, preview_table, query.
- Never invent table or column names; discover them with tools first.
- Prefer small exploratory queries before large ones.
- When multiple databases may be involved, inspect each relevant connection.
- Final answers must be Markdown: summary, evidence (tables/SQL), and caveats.
"#;

fn validate_agent_id(agent_id: &str) -> AppResult<()> {
    if agent_id.is_empty()
        || agent_id.contains('/')
        || agent_id.contains('\\')
        || agent_id.contains("..")
    {
        return Err(AppError::msg("invalid agent id"));
    }
    Ok(())
}

fn validate_session_id(session_id: &str) -> AppResult<()> {
    if session_id.is_empty()
        || session_id.contains('/')
        || session_id.contains('\\')
        || session_id.contains("..")
    {
        return Err(AppError::msg("invalid session id"));
    }
    Ok(())
}

fn agent_dir(agents_root: &Path, agent_id: &str) -> AppResult<PathBuf> {
    validate_agent_id(agent_id)?;
    let dir = agents_root.join(agent_id);
    if !dir.is_dir() {
        return Err(AppError::msg("agent not found"));
    }
    Ok(dir)
}

fn session_dir(agents_root: &Path, agent_id: &str, session_id: &str) -> AppResult<PathBuf> {
    validate_session_id(session_id)?;
    Ok(agent_dir(agents_root, agent_id)?.join("sessions").join(session_id))
}

fn clip(value: &str, max: usize) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= max {
        return trimmed.to_string();
    }
    trimmed.chars().take(max).collect()
}

pub fn list_agents(agents_root: &Path) -> AppResult<Vec<AgentInfo>> {
    ensure_builtins(agents_root)?;
    let mut items = Vec::new();
    if !agents_root.is_dir() {
        return Ok(items);
    }
    for entry in fs::read_dir(agents_root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let path = entry.path().join("agent.json");
        if !path.is_file() {
            continue;
        }
        let raw = fs::read_to_string(path)?;
        if let Ok(info) = serde_json::from_str::<AgentInfo>(&raw) {
            items.push(info);
        }
    }
    items.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(items)
}

pub fn read_system_prompt(agents_root: &Path, agent_id: &str) -> AppResult<String> {
    let path = agent_dir(agents_root, agent_id)?.join("system.md");
    if !path.is_file() {
        return Ok(DEFAULT_SYSTEM_MD.to_string());
    }
    Ok(fs::read_to_string(path)?)
}

pub fn list_sessions(agents_root: &Path, agent_id: &str) -> AppResult<Vec<AgentSessionMeta>> {
    let sessions = agent_dir(agents_root, agent_id)?.join("sessions");
    if !sessions.is_dir() {
        return Ok(Vec::new());
    }
    let mut items = Vec::new();
    for entry in fs::read_dir(sessions)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let meta_path = entry.path().join("meta.json");
        if !meta_path.is_file() {
            continue;
        }
        let raw = fs::read_to_string(meta_path)?;
        if let Ok(meta) = serde_json::from_str::<AgentSessionMeta>(&raw) {
            items.push(meta);
        }
    }
    items.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    if items.len() > MAX_SESSIONS {
        items.truncate(MAX_SESSIONS);
    }
    Ok(items)
}

pub fn create_session(
    agents_root: &Path,
    agent_id: &str,
    request: NewAgentSession,
) -> AppResult<AgentSessionMeta> {
    let title = clip(&request.title, MAX_TITLE);
    if title.is_empty() {
        return Err(AppError::msg("session title is empty"));
    }
    let root = agent_dir(agents_root, agent_id)?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    let dir = root.join("sessions").join(&id);
    fs::create_dir_all(&dir)?;
    let meta = AgentSessionMeta {
        id,
        agent_id: agent_id.to_string(),
        title,
        created_at: now.clone(),
        updated_at: now,
        status: "active".into(),
    };
    fs::write(dir.join("meta.json"), serde_json::to_vec_pretty(&meta)?)?;
    fs::write(dir.join("transcript.jsonl"), b"")?;
    Ok(meta)
}

pub fn load_session(
    agents_root: &Path,
    agent_id: &str,
    session_id: &str,
) -> AppResult<AgentSessionDetail> {
    let dir = session_dir(agents_root, agent_id, session_id)?;
    let meta_raw = fs::read_to_string(dir.join("meta.json"))?;
    let meta: AgentSessionMeta = serde_json::from_str(&meta_raw)?;
    let transcript = read_transcript(&dir.join("transcript.jsonl"))?;
    let checkpoint = match fs::read_to_string(dir.join("checkpoint.json")) {
        Ok(raw) if !raw.trim().is_empty() => Some(serde_json::from_str(&raw)?),
        _ => None,
    };
    Ok(AgentSessionDetail {
        meta,
        transcript,
        checkpoint,
    })
}

fn read_transcript(path: &Path) -> AppResult<Vec<TranscriptEntry>> {
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(path)?;
    let mut entries = Vec::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<TranscriptEntry>(line) {
            entries.push(entry);
        }
    }
    Ok(entries)
}

pub fn append_transcript(
    agents_root: &Path,
    agent_id: &str,
    session_id: &str,
    entry: TranscriptEntry,
) -> AppResult<TranscriptEntry> {
    let dir = session_dir(agents_root, agent_id, session_id)?;
    let mut entry = entry;
    if entry.id.trim().is_empty() {
        entry.id = uuid::Uuid::new_v4().to_string();
    }
    if entry.at.trim().is_empty() {
        entry.at = chrono::Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    }
    entry.content = entry.content.chars().take(200_000).collect();
    let line = serde_json::to_string(&entry)?;
    if line.len() > MAX_ENTRY_BYTES {
        return Err(AppError::msg("transcript entry is too large"));
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("transcript.jsonl"))?;
    writeln!(file, "{line}")?;

    touch_meta(&dir, None)?;
    Ok(entry)
}

pub fn save_checkpoint(
    agents_root: &Path,
    agent_id: &str,
    session_id: &str,
    checkpoint: Value,
    status: Option<String>,
) -> AppResult<()> {
    let dir = session_dir(agents_root, agent_id, session_id)?;
    fs::write(
        dir.join("checkpoint.json"),
        serde_json::to_vec_pretty(&checkpoint)?,
    )?;
    touch_meta(&dir, status.as_deref())?;
    Ok(())
}

fn touch_meta(dir: &Path, status: Option<&str>) -> AppResult<()> {
    let path = dir.join("meta.json");
    let raw = fs::read_to_string(&path)?;
    let mut meta: AgentSessionMeta = serde_json::from_str(&raw)?;
    meta.updated_at = chrono::Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    if let Some(status) = status {
        let clipped = clip(status, 40);
        if !clipped.is_empty() {
            meta.status = clipped;
        }
    }
    fs::write(path, serde_json::to_vec_pretty(&meta)?)?;
    Ok(())
}
