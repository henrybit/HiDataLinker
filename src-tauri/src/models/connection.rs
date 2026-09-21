use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineKind {
    MySql,
    Postgres,
}

impl EngineKind {
    pub fn parse(value: &str) -> AppResult<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" | "mysql" | "mariadb" => Ok(Self::MySql),
            "postgres" | "postgresql" | "pgsql" => Ok(Self::Postgres),
            other => Err(AppError::msg(format!("unsupported engine: {other}"))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::MySql => "mysql",
            Self::Postgres => "postgres",
        }
    }

    pub fn default_port(self) -> u16 {
        match self {
            Self::MySql => 3306,
            Self::Postgres => 5432,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionProfile {
    pub id: String,
    pub name: String,
    pub engine: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub ssl_ca: Option<String>,
    #[serde(default)]
    pub ssl_cert: Option<String>,
    #[serde(default)]
    pub ssl_key: Option<String>,
    #[serde(default)]
    pub save_password: bool,
}

impl ConnectionProfile {
    pub fn sanitized(&self) -> Self {
        let mut clone = self.clone();
        if !clone.save_password {
            clone.password = None;
        }
        clone
    }

    pub fn engine_kind(&self) -> AppResult<EngineKind> {
        EngineKind::parse(&self.engine)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionListItem {
    #[serde(flatten)]
    pub profile: ConnectionProfile,
    pub connected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConnectResult {
    pub evicted_ids: Vec<String>,
}

fn default_engine() -> String {
    "mysql".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestConnectionRequest {
    #[serde(default = "default_engine")]
    pub engine: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: Option<String>,
    pub database: Option<String>,
    #[serde(default)]
    pub ssl_ca: Option<String>,
    #[serde(default)]
    pub ssl_cert: Option<String>,
    #[serde(default)]
    pub ssl_key: Option<String>,
}

impl TestConnectionRequest {
    pub fn engine_kind(&self) -> AppResult<EngineKind> {
        EngineKind::parse(&self.engine)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_engine_aliases() {
        assert_eq!(EngineKind::parse("mysql").unwrap(), EngineKind::MySql);
        assert_eq!(
            EngineKind::parse("PostgreSQL").unwrap(),
            EngineKind::Postgres
        );
        assert_eq!(EngineKind::parse("pgsql").unwrap(), EngineKind::Postgres);
        assert!(EngineKind::parse("oracle").is_err());
    }

    #[test]
    fn deserializes_profile_without_ssl_fields() {
        let json = r#"{
            "id":"1",
            "name":"local",
            "engine":"mysql",
            "host":"127.0.0.1",
            "port":3306,
            "username":"root",
            "savePassword":true
        }"#;
        let profile: ConnectionProfile = serde_json::from_str(json).unwrap();
        assert!(profile.ssl_ca.is_none());
        assert!(profile.ssl_cert.is_none());
        assert!(profile.ssl_key.is_none());
    }
}
