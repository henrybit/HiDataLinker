use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering};
use std::sync::Mutex;

const AUDIT_LIMIT: usize = 40;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntry {
    pub id: u64,
    pub at: String,
    pub tool: String,
    pub connection_id: Option<String>,
    pub summary: String,
    pub ok: bool,
    pub duration_ms: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStatus {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub running: bool,
}

#[derive(Serialize, Deserialize)]
struct AgentSettings {
    enabled: bool,
}

pub struct AgentHub {
    token: String,
    port: AtomicU16,
    enabled: AtomicBool,
    audit: Mutex<VecDeque<AuditEntry>>,
    next_id: AtomicU64,
    settings_path: PathBuf,
}

impl AgentHub {
    pub fn new(token: String, enabled: bool, settings_path: PathBuf) -> Self {
        Self {
            token,
            port: AtomicU16::new(0),
            enabled: AtomicBool::new(enabled),
            audit: Mutex::new(VecDeque::new()),
            next_id: AtomicU64::new(1),
            settings_path,
        }
    }

    pub fn load_enabled(path: &std::path::Path) -> bool {
        let Ok(raw) = std::fs::read_to_string(path) else {
            return false;
        };
        serde_json::from_str::<AgentSettings>(&raw)
            .map(|settings| settings.enabled)
            .unwrap_or(false)
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn port(&self) -> u16 {
        self.port.load(Ordering::Relaxed)
    }

    pub fn set_port(&self, port: u16) {
        self.port.store(port, Ordering::Relaxed);
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn set_enabled(&self, enabled: bool) -> Result<(), String> {
        self.enabled.store(enabled, Ordering::Relaxed);
        self.persist_enabled(enabled)
    }

    pub fn status(&self) -> AgentStatus {
        let port = self.port();
        AgentStatus {
            enabled: self.enabled(),
            host: "127.0.0.1".into(),
            port,
            running: port != 0,
        }
    }

    pub fn record(
        &self,
        tool: String,
        connection_id: Option<String>,
        summary: String,
        ok: bool,
        duration_ms: u64,
        error: Option<String>,
    ) -> AuditEntry {
        let entry = AuditEntry {
            id: self.next_id.fetch_add(1, Ordering::Relaxed),
            at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            tool,
            connection_id,
            summary,
            ok,
            duration_ms,
            error,
        };
        let mut audit = self.audit.lock().unwrap_or_else(|error| error.into_inner());
        audit.push_front(entry.clone());
        while audit.len() > AUDIT_LIMIT {
            audit.pop_back();
        }
        entry
    }

    pub fn recent(&self) -> Vec<AuditEntry> {
        self.audit
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .cloned()
            .collect()
    }

    pub fn tokens_match(&self, given: &str) -> bool {
        let expected = self.token.as_bytes();
        let given = given.as_bytes();
        if expected.len() != given.len() {
            return false;
        }
        let mut diff = 0u8;
        for (left, right) in expected.iter().zip(given.iter()) {
            diff |= left ^ right;
        }
        diff == 0
    }

    fn persist_enabled(&self, enabled: bool) -> Result<(), String> {
        if let Some(parent) = self.settings_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let body = serde_json::to_string_pretty(&AgentSettings { enabled })
            .map_err(|error| error.to_string())?;
        std::fs::write(&self.settings_path, body).map_err(|error| error.to_string())
    }
}
