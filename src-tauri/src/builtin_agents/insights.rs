use crate::error::AppResult;
use serde_json::Value;
use std::path::{Path, PathBuf};

pub const INSIGHTS_AGENT_ID: &str = "insights";

fn latest_path(dir: &Path) -> PathBuf {
    dir.join("latest.json")
}

pub fn read_latest_report(dir: &Path) -> AppResult<Option<Value>> {
    let path = latest_path(dir);
    if !path.is_file() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&path)?;
    match serde_json::from_str::<Value>(&raw) {
        Ok(value) => Ok(Some(value)),
        Err(error) => {
            log::warn!("insights report unreadable, ignoring: {error}");
            Ok(None)
        }
    }
}

pub fn write_latest_report(dir: &Path, report: &Value) -> AppResult<()> {
    std::fs::create_dir_all(dir)?;
    let path = latest_path(dir);
    let tmp = dir.join("latest.json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(report)?)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn round_trips_latest_report() {
        let dir = std::env::temp_dir().join(format!("hdl-insights-{}", uuid::Uuid::new_v4()));
        assert!(read_latest_report(&dir).unwrap().is_none());
        let report = json!({ "generatedAt": "2026-11-10T00:00:00Z", "items": [{ "id": "1" }] });
        write_latest_report(&dir, &report).unwrap();
        assert_eq!(read_latest_report(&dir).unwrap(), Some(report));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
