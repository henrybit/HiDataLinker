use crate::error::{AppError, AppResult};
use crate::paths::AppPaths;
use chrono::{Duration, Local, NaiveDate};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub const RETAIN_DAYS: i64 = 7;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeLogSettings {
    /// When true, append logs to ~/.hidatalinker/runtime/YYYY-MM-DD.log
    pub file_logging_enabled: bool,
}

impl Default for RuntimeLogSettings {
    fn default() -> Self {
        Self {
            file_logging_enabled: true,
        }
    }
}

#[derive(Clone)]
pub struct RuntimeLogController {
    enabled: Arc<AtomicBool>,
    settings_path: PathBuf,
}

impl RuntimeLogController {
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn settings(&self) -> RuntimeLogSettings {
        RuntimeLogSettings {
            file_logging_enabled: self.is_enabled(),
        }
    }

    pub fn set_enabled(&self, enabled: bool) -> AppResult<()> {
        self.enabled.store(enabled, Ordering::Relaxed);
        let settings = RuntimeLogSettings {
            file_logging_enabled: enabled,
        };
        save_settings(&self.settings_path, &settings)
    }
}

pub fn load_settings(path: &Path) -> AppResult<RuntimeLogSettings> {
    if !path.exists() {
        return Ok(RuntimeLogSettings::default());
    }
    let raw = fs::read_to_string(path)?;
    if raw.trim().is_empty() {
        return Ok(RuntimeLogSettings::default());
    }
    Ok(serde_json::from_str(&raw)?)
}

pub fn save_settings(path: &Path, settings: &RuntimeLogSettings) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(settings)?)?;
    Ok(())
}

/// Delete `YYYY-MM-DD.log` files older than `RETAIN_DAYS`.
pub fn cleanup_old_logs(runtime_dir: &Path) -> AppResult<usize> {
    if !runtime_dir.is_dir() {
        return Ok(0);
    }
    let today = Local::now().date_naive();
    let cutoff = today - Duration::days(RETAIN_DAYS);
    let mut removed = 0usize;
    for entry in fs::read_dir(runtime_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(date) = parse_daily_log_name(name) else {
            continue;
        };
        if date < cutoff {
            fs::remove_file(entry.path())?;
            removed += 1;
        }
    }
    Ok(removed)
}

fn parse_daily_log_name(name: &str) -> Option<NaiveDate> {
    let stem = name.strip_suffix(".log")?;
    NaiveDate::parse_from_str(stem, "%Y-%m-%d").ok()
}

/// Install the global logger (stdout in debug + optional daily file under runtime/).
pub fn init(paths: &AppPaths) -> AppResult<RuntimeLogController> {
    fs::create_dir_all(&paths.runtime)?;
    match cleanup_old_logs(&paths.runtime) {
        Ok(0) => {}
        Ok(n) => eprintln!("[runtime-log] removed {n} expired log file(s)"),
        Err(error) => eprintln!(
            "[runtime-log] cleanup failed: {}",
            error.user_message()
        ),
    }

    let settings = load_settings(&paths.runtime_settings).unwrap_or_default();
    let enabled = Arc::new(AtomicBool::new(settings.file_logging_enabled));
    let controller = RuntimeLogController {
        enabled: Arc::clone(&enabled),
        settings_path: paths.runtime_settings.clone(),
    };

    let mut dispatch = fern::Dispatch::new()
        .level(log::LevelFilter::Info)
        .format(|out, message, record| {
            out.finish(format_args!(
                "[{}][{}][{}] {}",
                Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
                record.level(),
                record.target(),
                message
            ))
        });

    if cfg!(debug_assertions) {
        dispatch = dispatch.chain(std::io::stdout());
    }

    let prefix = format!("{}{}", paths.runtime.display(), std::path::MAIN_SEPARATOR);
    let file_dispatch = fern::Dispatch::new()
        .filter({
            let enabled = Arc::clone(&enabled);
            move |_meta| enabled.load(Ordering::Relaxed)
        })
        .chain(fern::DateBased::new(prefix, "%Y-%m-%d.log"));
    dispatch = dispatch.chain(file_dispatch);

    dispatch
        .apply()
        .map_err(|error| AppError::msg(format!("failed to install runtime logger: {error}")))?;

    log::info!(
        "runtime logging ready (file_logging_enabled={}, dir={})",
        settings.file_logging_enabled,
        paths.runtime.display()
    );
    Ok(controller)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_daily_names() {
        assert_eq!(
            parse_daily_log_name("2026-11-02.log"),
            Some(NaiveDate::from_ymd_opt(2026, 11, 2).unwrap())
        );
        assert!(parse_daily_log_name("notes.txt").is_none());
    }

    #[test]
    fn cleanup_removes_older_than_seven_days() {
        let root = std::env::temp_dir().join(format!("hdl-runtime-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let today = Local::now().date_naive();
        let keep = (today - Duration::days(3)).format("%Y-%m-%d").to_string();
        let drop = (today - Duration::days(8)).format("%Y-%m-%d").to_string();
        fs::write(root.join(format!("{keep}.log")), "keep").unwrap();
        fs::write(root.join(format!("{drop}.log")), "drop").unwrap();
        fs::write(root.join("readme.txt"), "ignore").unwrap();
        let removed = cleanup_old_logs(&root).unwrap();
        assert_eq!(removed, 1);
        assert!(root.join(format!("{keep}.log")).is_file());
        assert!(!root.join(format!("{drop}.log")).exists());
        assert!(root.join("readme.txt").is_file());
        let _ = fs::remove_dir_all(root);
    }
}
