use crate::error::{AppError, AppResult};
use std::path::{Path, PathBuf};

/// Layout under the user's home directory.
///
/// ```text
/// ~/.hidatalinker/
///   connections/connections.json
///   queries/
///   analysis/
///   llm/providers.json
///   settings/locale
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppPaths {
    pub root: PathBuf,
    pub connections: PathBuf,
    pub queries: PathBuf,
    pub analysis: PathBuf,
    pub llm: PathBuf,
    pub locale: PathBuf,
}

impl AppPaths {
    pub fn from_home(home: &Path) -> Self {
        Self::from_root(home.join(".hidatalinker"))
    }

    pub fn from_root(root: PathBuf) -> Self {
        Self {
            connections: root.join("connections").join("connections.json"),
            queries: root.join("queries"),
            analysis: root.join("analysis"),
            llm: root.join("llm").join("providers.json"),
            locale: root.join("settings").join("locale"),
            root,
        }
    }

    pub fn ensure(&self) -> AppResult<()> {
        for dir in [
            self.connections.parent(),
            Some(self.queries.as_path()),
            Some(self.analysis.as_path()),
            self.llm.parent(),
            self.locale.parent(),
        ] {
            if let Some(dir) = dir {
                std::fs::create_dir_all(dir)?;
            }
        }
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
            paths.llm,
            PathBuf::from("/Users/ada/.hidatalinker/llm/providers.json")
        );
        assert_eq!(
            paths.locale,
            PathBuf::from("/Users/ada/.hidatalinker/settings/locale")
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
}
