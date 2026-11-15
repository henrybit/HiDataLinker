use crate::db::{DatabaseEngine, LiveEngine};
use crate::error::{AppError, AppResult};
use crate::models::{ConnectResult, ConnectionProfile, EngineKind};
use crate::paths::{migrate_analysis, migrate_file, AppPaths};
use crate::runtime::{run_blocking, run_db};
use crate::sql_audit::{AuditConnection, AuditScope, SqlAuditLog};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tokio::sync::RwLock;

pub const MAX_OPEN_SESSIONS: usize = 5;

#[derive(Serialize, Deserialize, Default)]
struct StoredProfiles {
    connections: Vec<ConnectionProfile>,
}

pub struct AppState {
    data_file: PathBuf,
    /// Legacy Application Support copy; cleared after a successful home persist
    /// so a missing home file cannot remigrate stale profiles.
    legacy_connections: Option<PathBuf>,
    query_cache_dir: PathBuf,
    analysis_history_dir: PathBuf,
    migration_history_dir: PathBuf,
    llm_catalog_path: PathBuf,
    locale_path: PathBuf,
    agents_dir: PathBuf,
    chats_dir: PathBuf,
    temp_dir: PathBuf,
    insights_dir: PathBuf,
    schema_index_path: PathBuf,
    schema_cache_dir: PathBuf,
    sql_audit: SqlAuditLog,
    profiles: RwLock<Vec<ConnectionProfile>>,
    sessions: DashMap<String, LiveEngine>,
    /// Profile each live session was opened with, including a prompted
    /// password and a database switched to after connecting.
    session_profiles: DashMap<String, ConnectionProfile>,
    session_order: Mutex<Vec<String>>,
    connect_lock: tokio::sync::Mutex<()>,
    schema_refresh_lock: tokio::sync::Mutex<()>,
    /// Serializes profile mutate + disk write so a slower upsert cannot
    /// overwrite a later delete (lost-update on connections.json).
    persist_lock: tokio::sync::Mutex<()>,
}

impl AppState {
    pub fn load(paths: AppPaths, legacy_dir: Option<PathBuf>) -> AppResult<Self> {
        paths.ensure()?;
        let legacy_connections = legacy_dir
            .as_ref()
            .map(|dir| dir.join("connections.json"));
        if let Some(legacy) = legacy_connections.as_ref() {
            migrate_file(legacy, &paths.connections)?;
        }
        if let Some(legacy) = legacy_dir.as_deref() {
            migrate_analysis(&legacy.join("analysis-history"), &paths.analysis)?;
        }
        let data_file = paths.connections;
        let query_cache_dir = paths.queries;
        reset_query_cache_dir(&query_cache_dir)?;
        let analysis_history_dir = paths.analysis;
        let migration_history_dir = paths.migrations;
        let llm_catalog_path = paths.llm;
        let locale_path = paths.locale;
        let agents_dir = paths.agents;
        let chats_dir = paths.chats;
        let temp_dir = paths.temp;
        let insights_dir = paths.insights;
        let schema_index_path = paths.db_index;
        let schema_cache_dir = paths.db_schemas;
        let sql_audit = SqlAuditLog::open(paths.audit, paths.audit_settings)?;
        let profiles = if data_file.exists() {
            let raw = std::fs::read_to_string(&data_file)?;
            serde_json::from_str::<StoredProfiles>(&raw)?.connections
        } else {
            Vec::new()
        };

        Ok(Self {
            data_file,
            legacy_connections,
            query_cache_dir,
            analysis_history_dir,
            migration_history_dir,
            llm_catalog_path,
            locale_path,
            agents_dir,
            chats_dir,
            temp_dir,
            insights_dir,
            schema_index_path,
            schema_cache_dir,
            sql_audit,
            profiles: RwLock::new(profiles),
            sessions: DashMap::new(),
            session_profiles: DashMap::new(),
            session_order: Mutex::new(Vec::new()),
            connect_lock: tokio::sync::Mutex::new(()),
            schema_refresh_lock: tokio::sync::Mutex::new(()),
            persist_lock: tokio::sync::Mutex::new(()),
        })
    }

    pub async fn list_profiles(&self) -> Vec<ConnectionProfile> {
        self.profiles.read().await.clone()
    }

    pub async fn upsert_profile(
        &self,
        mut profile: ConnectionProfile,
    ) -> AppResult<ConnectionProfile> {
        if profile.name.trim().is_empty() {
            return Err(AppError::msg("connection name is required"));
        }
        if profile.host.trim().is_empty() {
            return Err(AppError::msg("host is required"));
        }
        let engine_kind = profile.engine_kind()?;
        profile.engine = engine_kind.as_str().to_string();
        if profile.port == 0 {
            profile.port = engine_kind.default_port();
        }
        if profile.id.trim().is_empty() {
            profile.id = uuid::Uuid::new_v4().to_string();
        }
        if !profile.save_password {
            profile.password = None;
        }
        profile.ssl_ca = trim_optional_path(profile.ssl_ca.take());
        profile.ssl_cert = trim_optional_path(profile.ssl_cert.take());
        profile.ssl_key = trim_optional_path(profile.ssl_key.take());

        let _persist = self.persist_lock.lock().await;
        {
            let mut profiles = self.profiles.write().await;
            if let Some(existing) = profiles.iter_mut().find(|item| item.id == profile.id) {
                *existing = profile.clone();
            } else {
                profiles.push(profile.clone());
            }
        }
        self.persist_locked().await?;
        Ok(profile)
    }

    pub async fn delete_profile(&self, id: &str) -> AppResult<()> {
        self.disconnect(id).await.ok();
        let _persist = self.persist_lock.lock().await;
        {
            let mut profiles = self.profiles.write().await;
            let before = profiles.len();
            profiles.retain(|item| item.id != id);
            if profiles.len() == before {
                return Err(AppError::ProfileNotFound(id.to_string()));
            }
        }
        self.persist_locked().await?;
        drop(_persist);
        let _ = crate::schema_cache::remove_connection(self, id).await;
        Ok(())
    }

    pub async fn get_profile(&self, id: &str) -> AppResult<ConnectionProfile> {
        self.profiles
            .read()
            .await
            .iter()
            .find(|item| item.id == id)
            .cloned()
            .ok_or_else(|| AppError::ProfileNotFound(id.to_string()))
    }

    pub async fn connect(
        &self,
        id: &str,
        password_override: Option<String>,
    ) -> AppResult<ConnectResult> {
        let _guard = self.connect_lock.lock().await;
        if self.sessions.contains_key(id) {
            return Ok(ConnectResult {
                evicted_ids: Vec::new(),
            });
        }
        let mut profile = self.get_profile(id).await?;
        if let Some(password) = password_override {
            profile.password = Some(password);
        }
        let engine = self.open_engine(profile.clone()).await?;

        let mut closing = Vec::new();
        let evicted_ids = {
            let mut order = self
                .session_order
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let evicted_ids = ids_to_evict(&order, id, MAX_OPEN_SESSIONS);
            for evict_id in &evicted_ids {
                order.retain(|existing| existing != evict_id);
                self.session_profiles.remove(evict_id);
                if let Some((_, old)) = self.sessions.remove(evict_id) {
                    closing.push(old);
                }
            }
            self.sessions.insert(id.to_string(), engine);
            self.session_profiles.insert(id.to_string(), profile);
            order.push(id.to_string());
            evicted_ids
        };
        for old in closing {
            let _ = run_db(async move { old.close().await }).await;
        }
        Ok(ConnectResult { evicted_ids })
    }

    pub async fn disconnect(&self, id: &str) -> AppResult<()> {
        {
            let mut order = self
                .session_order
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            order.retain(|existing| existing != id);
        }
        self.session_profiles.remove(id);
        let engine = self.sessions.remove(id).map(|(_, engine)| engine);
        if let Some(engine) = engine {
            run_db(async move { engine.close().await }).await?;
        }
        Ok(())
    }

    async fn open_engine(&self, profile: ConnectionProfile) -> AppResult<LiveEngine> {
        let audit = AuditScope::new(self.sql_audit.clone(), AuditConnection::from(&profile));
        run_db(async move {
            let engine = LiveEngine::from_profile(&profile)?.with_audit(audit);
            match engine.ping().await {
                Ok(()) => Ok(engine),
                Err(error) => {
                    let _ = engine.close().await;
                    Err(error)
                }
            }
        })
        .await
    }

    /// Reopen a live PostgreSQL session against another database on the same
    /// server. The saved profile is left unchanged.
    pub async fn switch_database(&self, id: &str, database: &str) -> AppResult<()> {
        let database = database.trim();
        if database.is_empty() {
            return Err(AppError::msg("database name is required"));
        }
        let _guard = self.connect_lock.lock().await;
        let mut profile = self
            .session_profiles
            .get(id)
            .map(|entry| entry.clone())
            .ok_or_else(|| AppError::NotConnected(id.to_string()))?;
        if profile.engine_kind()? != EngineKind::Postgres {
            return Err(AppError::msg(
                "switching databases is only supported for PostgreSQL connections",
            ));
        }
        profile.database = Some(database.to_string());
        let engine = self.open_engine(profile.clone()).await?;
        let previous = self.sessions.insert(id.to_string(), engine);
        self.session_profiles.insert(id.to_string(), profile);
        if let Some(previous) = previous {
            let _ = run_db(async move { previous.close().await }).await;
        }
        let _ = crate::schema_cache::remove_connection(self, id).await;
        Ok(())
    }

    /// Database the live session is using, when the engine connects to one.
    pub fn active_database(&self, id: &str) -> Option<String> {
        let profile = self.session_profiles.get(id)?;
        let database = profile
            .database
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        match (profile.engine_kind().ok()?, database) {
            (_, Some(database)) => Some(database.to_string()),
            (EngineKind::Postgres, None) => Some("postgres".to_string()),
            _ => None,
        }
    }

    pub fn is_connected(&self, id: &str) -> bool {
        self.sessions.contains_key(id)
    }

    pub fn engine(&self, id: &str) -> AppResult<LiveEngine> {
        self.sessions
            .get(id)
            .map(|entry| entry.clone())
            .ok_or_else(|| AppError::NotConnected(id.to_string()))
    }

    pub fn sql_audit(&self) -> &SqlAuditLog {
        &self.sql_audit
    }

    pub fn query_cache_dir(&self) -> &PathBuf {
        &self.query_cache_dir
    }

    pub fn analysis_history_dir(&self) -> &PathBuf {
        &self.analysis_history_dir
    }

    pub fn migration_history_dir(&self) -> &PathBuf {
        &self.migration_history_dir
    }

    pub fn agents_dir(&self) -> &PathBuf {
        &self.agents_dir
    }

    pub fn chats_dir(&self) -> &PathBuf {
        &self.chats_dir
    }

    pub fn temp_dir(&self) -> &PathBuf {
        &self.temp_dir
    }

    pub fn insights_dir(&self) -> &PathBuf {
        &self.insights_dir
    }

    pub fn schema_index_path(&self) -> &PathBuf {
        &self.schema_index_path
    }

    pub fn schema_cache_dir(&self) -> &PathBuf {
        &self.schema_cache_dir
    }

    pub fn schema_refresh_lock(&self) -> &tokio::sync::Mutex<()> {
        &self.schema_refresh_lock
    }

    pub fn llm_catalog_path(&self) -> &PathBuf {
        &self.llm_catalog_path
    }

    pub fn locale_path(&self) -> &PathBuf {
        &self.locale_path
    }

    /// Caller must hold `persist_lock`.
    async fn persist_locked(&self) -> AppResult<()> {
        let profiles = self.profiles.read().await;
        let stored = StoredProfiles {
            connections: profiles.iter().map(ConnectionProfile::sanitized).collect(),
        };
        drop(profiles);
        let json = serde_json::to_string_pretty(&stored)?;
        let path = self.data_file.clone();
        let legacy = self.legacy_connections.clone();
        run_blocking(move || {
            atomic_write(&path, json.as_bytes())?;
            if let Some(legacy) = legacy {
                // Drop the one-shot migrate source so a deleted home file cannot
                // resurrect profiles that the user already removed.
                if legacy.is_file() {
                    let _ = std::fs::remove_file(legacy);
                }
            }
            Ok(())
        })
        .await
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> AppResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::msg("connections path has no parent directory"))?;
    std::fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("connections.json"),
        std::process::id()
    ));
    std::fs::write(&tmp, bytes)?;
    if path.exists() {
        // Windows cannot rename over an existing file.
        let _ = std::fs::remove_file(path);
    }
    std::fs::rename(&tmp, path).map_err(|error| {
        let _ = std::fs::remove_file(&tmp);
        AppError::from(error)
    })?;
    Ok(())
}

fn trim_optional_path(value: Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| item.to_string())
}

fn reset_query_cache_dir(dir: &PathBuf) -> AppResult<()> {
    if dir.exists() {
        std::fs::remove_dir_all(dir)?;
    }
    std::fs::create_dir_all(dir)?;
    Ok(())
}

fn ids_to_evict(open_order: &[String], connecting_id: &str, max: usize) -> Vec<String> {
    if open_order.iter().any(|id| id == connecting_id) {
        return Vec::new();
    }
    let keep_existing = max.saturating_sub(1);
    open_order
        .iter()
        .take(open_order.len().saturating_sub(keep_existing))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::AppPaths;

    fn sample_profile(id: &str, name: &str) -> ConnectionProfile {
        ConnectionProfile {
            id: id.into(),
            name: name.into(),
            engine: "mysql".into(),
            host: "127.0.0.1".into(),
            port: 3306,
            username: "root".into(),
            password: Some("secret".into()),
            database: None,
            ssl_ca: None,
            ssl_cert: None,
            ssl_key: None,
            ssl_verify: false,
            oracle_version: None,
            oracle_connect: None,
            oracle_instant_client: None,
            save_password: true,
        }
    }

    #[test]
    fn keeps_sessions_under_the_cap() {
        let open = vec!["a".into(), "b".into()];
        assert!(ids_to_evict(&open, "c", 5).is_empty());
    }

    #[test]
    fn reconnect_does_not_evict() {
        let open = vec!["a".into(), "b".into(), "c".into(), "d".into(), "e".into()];
        assert!(ids_to_evict(&open, "c", 5).is_empty());
    }

    #[test]
    fn sixth_connection_closes_the_oldest() {
        let open = vec!["a".into(), "b".into(), "c".into(), "d".into(), "e".into()];
        assert_eq!(ids_to_evict(&open, "f", 5), vec!["a".to_string()]);
    }

    #[test]
    fn overflow_closes_multiple_oldest() {
        let open = vec![
            "a".into(),
            "b".into(),
            "c".into(),
            "d".into(),
            "e".into(),
            "f".into(),
        ];
        assert_eq!(
            ids_to_evict(&open, "g", 5),
            vec!["a".to_string(), "b".to_string()]
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn deleted_connection_stays_gone_after_reload() {
        let root = std::env::temp_dir().join(format!(
            "hidatalinker-conn-delete-{}",
            uuid::Uuid::new_v4()
        ));
        let legacy = root.join("legacy");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(
            legacy.join("connections.json"),
            r#"{"connections":[{"id":"legacy-1","name":"Legacy","engine":"mysql","host":"127.0.0.1","port":3306,"username":"root","savePassword":false}]}"#,
        )
        .unwrap();
        let paths = AppPaths::from_root(root.join("home").join(".hidatalinker"));
        let state = AppState::load(paths.clone(), Some(legacy.clone())).unwrap();
        let created = state
            .upsert_profile(sample_profile("", "Temp"))
            .await
            .unwrap();
        assert!(!created.id.is_empty());
        state.delete_profile(&created.id).await.unwrap();

        let reloaded = AppState::load(paths, Some(legacy)).unwrap();
        let ids: Vec<_> = reloaded
            .list_profiles()
            .await
            .into_iter()
            .map(|item| item.id)
            .collect();
        assert!(
            !ids.contains(&created.id),
            "deleted id still present after reload: {ids:?}"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_upsert_cannot_revive_a_delete() {
        let root = std::env::temp_dir().join(format!(
            "hidatalinker-conn-race-{}",
            uuid::Uuid::new_v4()
        ));
        let paths = AppPaths::from_root(root.join(".hidatalinker"));
        let state = std::sync::Arc::new(AppState::load(paths.clone(), None).unwrap());
        let kept = state
            .upsert_profile(sample_profile("keep", "Keep"))
            .await
            .unwrap();

        let state_upsert = state.clone();
        let upsert = tokio::spawn(async move {
            for i in 0..20 {
                let _ = state_upsert
                    .upsert_profile(sample_profile("", &format!("Temp-{i}")))
                    .await;
            }
        });

        // Let a few creates land, then delete the stable profile repeatedly is wrong —
        // instead create one disposable id and delete it while creates race.
        let doomed = state
            .upsert_profile(sample_profile("doomed", "Doomed"))
            .await
            .unwrap();
        let state_delete = state.clone();
        let doomed_id = doomed.id.clone();
        let delete = tokio::spawn(async move {
            state_delete.delete_profile(&doomed_id).await.unwrap();
        });

        let _ = tokio::join!(upsert, delete);
        // Ensure a final persist barrier by touching an upsert on the kept profile.
        state
            .upsert_profile(sample_profile(&kept.id, "Keep"))
            .await
            .unwrap();

        let reloaded = AppState::load(paths, None).unwrap();
        let ids: Vec<_> = reloaded
            .list_profiles()
            .await
            .into_iter()
            .map(|item| item.id)
            .collect();
        assert!(
            !ids.contains(&doomed.id),
            "doomed id revived after concurrent upserts: {ids:?}"
        );
        assert!(ids.contains(&kept.id));
        let _ = std::fs::remove_dir_all(root);
    }
}
