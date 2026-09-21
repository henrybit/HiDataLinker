use crate::db::{DatabaseEngine, LiveEngine};
use crate::error::{AppError, AppResult};
use crate::models::{ConnectResult, ConnectionProfile};
use crate::runtime::{run_blocking, run_db};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use tokio::sync::RwLock;

pub const MAX_OPEN_SESSIONS: usize = 5;

#[derive(Serialize, Deserialize, Default)]
struct StoredProfiles {
    connections: Vec<ConnectionProfile>,
}

pub struct AppState {
    data_file: PathBuf,
    query_cache_dir: PathBuf,
    profiles: RwLock<Vec<ConnectionProfile>>,
    sessions: DashMap<String, LiveEngine>,
    session_order: Mutex<Vec<String>>,
    connect_lock: tokio::sync::Mutex<()>,
}

impl AppState {
    pub fn load(data_dir: PathBuf) -> AppResult<Self> {
        std::fs::create_dir_all(&data_dir)?;
        let data_file = data_dir.join("connections.json");
        let query_cache_dir = data_dir.join("query-cache");
        reset_query_cache_dir(&query_cache_dir)?;
        let profiles = if data_file.exists() {
            let raw = std::fs::read_to_string(&data_file)?;
            serde_json::from_str::<StoredProfiles>(&raw)?.connections
        } else {
            Vec::new()
        };

        Ok(Self {
            data_file,
            query_cache_dir,
            profiles: RwLock::new(profiles),
            sessions: DashMap::new(),
            session_order: Mutex::new(Vec::new()),
            connect_lock: tokio::sync::Mutex::new(()),
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

        {
            let mut profiles = self.profiles.write().await;
            if let Some(existing) = profiles.iter_mut().find(|item| item.id == profile.id) {
                *existing = profile.clone();
            } else {
                profiles.push(profile.clone());
            }
        }
        self.persist().await?;
        Ok(profile)
    }

    pub async fn delete_profile(&self, id: &str) -> AppResult<()> {
        self.disconnect(id).await.ok();
        {
            let mut profiles = self.profiles.write().await;
            let before = profiles.len();
            profiles.retain(|item| item.id != id);
            if profiles.len() == before {
                return Err(AppError::ProfileNotFound(id.to_string()));
            }
        }
        self.persist().await
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
        let engine = run_db(async move {
            let engine = LiveEngine::from_profile(&profile)?;
            match engine.ping().await {
                Ok(()) => Ok(engine),
                Err(error) => {
                    let _ = engine.close().await;
                    Err(error)
                }
            }
        })
        .await?;

        let mut closing = Vec::new();
        let evicted_ids = {
            let mut order = self
                .session_order
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let evicted_ids = ids_to_evict(&order, id, MAX_OPEN_SESSIONS);
            for evict_id in &evicted_ids {
                order.retain(|existing| existing != evict_id);
                if let Some((_, old)) = self.sessions.remove(evict_id) {
                    closing.push(old);
                }
            }
            self.sessions.insert(id.to_string(), engine);
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
        let engine = self.sessions.remove(id).map(|(_, engine)| engine);
        if let Some(engine) = engine {
            run_db(async move { engine.close().await }).await?;
        }
        Ok(())
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

    pub fn query_cache_dir(&self) -> &PathBuf {
        &self.query_cache_dir
    }

    async fn persist(&self) -> AppResult<()> {
        let profiles = self.profiles.read().await;
        let stored = StoredProfiles {
            connections: profiles.iter().map(ConnectionProfile::sanitized).collect(),
        };
        drop(profiles);
        let json = serde_json::to_string_pretty(&stored)?;
        let path = self.data_file.clone();
        run_blocking(move || {
            std::fs::write(path, json)?;
            Ok(())
        })
        .await
    }
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
}
