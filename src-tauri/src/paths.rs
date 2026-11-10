use crate::error::{AppError, AppResult};
use std::path::{Path, PathBuf};

/// Layout under the user's home directory.
///
/// ```text
/// ~/.hidatalinker/
///   connections/connections.json
///   queries/
///   analysis/
///   migrations/
///   llm/providers.json
///   settings/locale
///   settings/agent.json
///   agent/endpoint.json
///   agents/<agentId>/…          # agent definition (system.md, agent.json)
///   chats/<agentId>/<sessionId>/ # chat persistence per session
///     meta.json
///     transcript.jsonl
///     checkpoint.json
///   db/index.json
///   db/schemas/<connectionId>/…
///   temp/scripts/<runId>/…     # agent-authored import/analysis scripts
///   insights/latest.json         # last insights-agent report
///   runtime/YYYY-MM-DD.log
///   settings/runtime.json
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppPaths {
    pub root: PathBuf,
    pub connections: PathBuf,
    pub queries: PathBuf,
    pub analysis: PathBuf,
    pub migrations: PathBuf,
    pub llm: PathBuf,
    pub locale: PathBuf,
    pub agent_settings: PathBuf,
    pub agent_endpoint: PathBuf,
    pub agents: PathBuf,
    /// Conversation transcripts: `chats/<agentId>/<sessionId>/`.
    pub chats: PathBuf,
    pub db: PathBuf,
    pub db_index: PathBuf,
    pub db_schemas: PathBuf,
    /// Ephemeral agent workspace: `temp/scripts/<runId>/`.
    pub temp: PathBuf,
    /// Insights agent reports: `insights/latest.json`.
    pub insights: PathBuf,
    pub runtime: PathBuf,
    pub runtime_settings: PathBuf,
}

impl AppPaths {
    pub fn from_home(home: &Path) -> Self {
        Self::from_root(home.join(".hidatalinker"))
    }

    pub fn from_root(root: PathBuf) -> Self {
        let db = root.join("db");
        Self {
            connections: root.join("connections").join("connections.json"),
            queries: root.join("queries"),
            analysis: root.join("analysis"),
            migrations: root.join("migrations"),
            llm: root.join("llm").join("providers.json"),
            locale: root.join("settings").join("locale"),
            agent_settings: root.join("settings").join("agent.json"),
            agent_endpoint: root.join("agent").join("endpoint.json"),
            agents: root.join("agents"),
            chats: root.join("chats"),
            db_index: db.join("index.json"),
            db_schemas: db.join("schemas"),
            db,
            temp: root.join("temp"),
            insights: root.join("insights"),
            runtime: root.join("runtime"),
            runtime_settings: root.join("settings").join("runtime.json"),
            root,
        }
    }

    pub fn ensure(&self) -> AppResult<()> {
        for dir in [
            self.connections.parent(),
            Some(self.queries.as_path()),
            Some(self.analysis.as_path()),
            Some(self.migrations.as_path()),
            self.llm.parent(),
            self.locale.parent(),
            self.agent_endpoint.parent(),
            Some(self.agents.as_path()),
            Some(self.chats.as_path()),
            Some(self.db.as_path()),
            Some(self.db_schemas.as_path()),
            Some(self.temp.as_path()),
            Some(self.insights.as_path()),
            Some(self.runtime.as_path()),
            self.runtime_settings.parent(),
        ] {
            if let Some(dir) = dir {
                std::fs::create_dir_all(dir)?;
            }
        }
        std::fs::create_dir_all(self.temp.join("scripts"))?;
        crate::builtin_agents::ensure_builtins(&self.agents, &self.chats)?;
        let _ = migrate_agent_sessions_to_chats(&self.agents, &self.chats);
        Ok(())
    }
}

pub fn home_dir() -> AppResult<PathBuf> {
    let value = if cfg!(windows) {
        std::env::var("USERPROFILE")
    } else {
        std::env::var("HOME")
    };
    let path = value.map_err(|_| AppError::msg("user home directory is not set"))?;
    let path = PathBuf::from(path);
    if path.as_os_str().is_empty() {
        return Err(AppError::msg("user home directory is not set"));
    }
    Ok(path)
}

/// Copy a previous app-data file when the home copy does not exist yet.
pub fn migrate_file(from: &Path, to: &Path) -> AppResult<()> {
    if to.exists() || !from.is_file() {
        return Ok(());
    }
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(from, to)?;
    Ok(())
}

/// Move legacy `agents/<agentId>/sessions/<sessionId>/` trees into
/// `chats/<agentId>/<sessionId>/` when the destination does not exist yet.
pub fn migrate_agent_sessions_to_chats(agents: &Path, chats: &Path) -> AppResult<()> {
    if !agents.is_dir() {
        return Ok(());
    }
    for agent_entry in std::fs::read_dir(agents)? {
        let agent_entry = agent_entry?;
        if !agent_entry.file_type()?.is_dir() {
            continue;
        }
        let agent_id = agent_entry.file_name();
        let Some(agent_id) = agent_id.to_str() else {
            continue;
        };
        let legacy_sessions = agent_entry.path().join("sessions");
        if !legacy_sessions.is_dir() {
            continue;
        }
        let dest_agent = chats.join(agent_id);
        std::fs::create_dir_all(&dest_agent)?;
        for session_entry in std::fs::read_dir(&legacy_sessions)? {
            let session_entry = session_entry?;
            if !session_entry.file_type()?.is_dir() {
                continue;
            }
            let session_id = session_entry.file_name();
            let dest = dest_agent.join(&session_id);
            if dest.exists() {
                continue;
            }
            std::fs::rename(session_entry.path(), &dest).or_else(|_| {
                copy_dir_recursive(&session_entry.path(), &dest)?;
                let _ = std::fs::remove_dir_all(session_entry.path());
                Ok::<(), AppError>(())
            })?;
        }
    }
    Ok(())
}

fn copy_dir_recursive(from: &Path, to: &Path) -> AppResult<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// Copy analysis history files from the old `analysis-history` directory.
pub fn migrate_analysis(from: &Path, to: &Path) -> AppResult<()> {
    if to.join("index.json").exists() || !from.is_dir() {
        return Ok(());
    }
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name != "index.json" && !name.ends_with(".json") {
            continue;
        }
        std::fs::copy(entry.path(), to.join(name))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_data_into_purpose_directories() {
        let paths = AppPaths::from_home(Path::new("/Users/ada"));
        assert_eq!(
            paths.connections,
            PathBuf::from("/Users/ada/.hidatalinker/connections/connections.json")
        );
        assert_eq!(
            paths.queries,
            PathBuf::from("/Users/ada/.hidatalinker/queries")
        );
        assert_eq!(
            paths.analysis,
            PathBuf::from("/Users/ada/.hidatalinker/analysis")
        );
        assert_eq!(
            paths.migrations,
            PathBuf::from("/Users/ada/.hidatalinker/migrations")
        );
        assert_eq!(
            paths.llm,
            PathBuf::from("/Users/ada/.hidatalinker/llm/providers.json")
        );
        assert_eq!(
            paths.locale,
            PathBuf::from("/Users/ada/.hidatalinker/settings/locale")
        );
        assert_eq!(
            paths.agent_settings,
            PathBuf::from("/Users/ada/.hidatalinker/settings/agent.json")
        );
        assert_eq!(
            paths.agent_endpoint,
            PathBuf::from("/Users/ada/.hidatalinker/agent/endpoint.json")
        );
        assert_eq!(
            paths.agents,
            PathBuf::from("/Users/ada/.hidatalinker/agents")
        );
        assert_eq!(
            paths.chats,
            PathBuf::from("/Users/ada/.hidatalinker/chats")
        );
        assert_eq!(paths.db, PathBuf::from("/Users/ada/.hidatalinker/db"));
        assert_eq!(
            paths.db_index,
            PathBuf::from("/Users/ada/.hidatalinker/db/index.json")
        );
        assert_eq!(
            paths.db_schemas,
            PathBuf::from("/Users/ada/.hidatalinker/db/schemas")
        );
        assert_eq!(
            paths.temp,
            PathBuf::from("/Users/ada/.hidatalinker/temp")
        );
        assert_eq!(
            paths.runtime,
            PathBuf::from("/Users/ada/.hidatalinker/runtime")
        );
        assert_eq!(
            paths.runtime_settings,
            PathBuf::from("/Users/ada/.hidatalinker/settings/runtime.json")
        );
    }

    #[test]
    fn migrates_legacy_files_once() {
        let root = std::env::temp_dir().join(format!("hidatalinker-{}", uuid::Uuid::new_v4()));
        let legacy = root.join("legacy");
        let home = root.join("home").join(".hidatalinker");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("connections.json"), "{\"connections\":[]}").unwrap();
        let history = legacy.join("analysis-history");
        std::fs::create_dir_all(&history).unwrap();
        std::fs::write(history.join("index.json"), "[]").unwrap();

        let paths = AppPaths::from_root(home);
        migrate_file(&legacy.join("connections.json"), &paths.connections).unwrap();
        migrate_analysis(&history, &paths.analysis).unwrap();
        assert_eq!(
            std::fs::read_to_string(&paths.connections).unwrap(),
            "{\"connections\":[]}"
        );
        assert!(paths.analysis.join("index.json").is_file());

        std::fs::write(&paths.connections, "kept").unwrap();
        std::fs::write(legacy.join("connections.json"), "old").unwrap();
        migrate_file(&legacy.join("connections.json"), &paths.connections).unwrap();
        assert_eq!(std::fs::read_to_string(&paths.connections).unwrap(), "kept");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn migrates_agent_sessions_into_chats() {
        let root = std::env::temp_dir().join(format!("hidatalinker-chats-{}", uuid::Uuid::new_v4()));
        let agents = root.join("agents");
        let chats = root.join("chats");
        let session = agents
            .join("data-analyst")
            .join("sessions")
            .join("sess-1");
        std::fs::create_dir_all(&session).unwrap();
        std::fs::write(session.join("meta.json"), "{\"id\":\"sess-1\"}").unwrap();
        std::fs::write(session.join("transcript.jsonl"), "{\"id\":\"m1\"}\n").unwrap();

        migrate_agent_sessions_to_chats(&agents, &chats).unwrap();
        let dest = chats.join("data-analyst").join("sess-1");
        assert!(dest.join("meta.json").is_file());
        assert!(dest.join("transcript.jsonl").is_file());
        assert!(!session.exists());

        // Second run keeps destination and is idempotent.
        std::fs::create_dir_all(&session).unwrap();
        std::fs::write(session.join("meta.json"), "old").unwrap();
        migrate_agent_sessions_to_chats(&agents, &chats).unwrap();
        assert_eq!(
            std::fs::read_to_string(dest.join("meta.json")).unwrap(),
            "{\"id\":\"sess-1\"}"
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
