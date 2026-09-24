//! XDG locations used by the application.

use std::path::PathBuf;

pub const APP_ID: &str = "io.github.jonas_bickel.Primvokon";
const DIR_NAME: &str = "primvokon";

/// `$XDG_CONFIG_HOME/primvokon`
pub fn config_dir() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join(DIR_NAME)
}

/// `$XDG_DATA_HOME/primvokon`
pub fn data_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join(DIR_NAME)
}

/// `$XDG_CACHE_HOME/primvokon`
pub fn cache_dir() -> PathBuf {
    dirs::cache_dir().unwrap_or_else(|| PathBuf::from(".")).join(DIR_NAME)
}

pub fn settings_file() -> PathBuf {
    config_dir().join("settings.toml")
}

pub fn database_file() -> PathBuf {
    data_dir().join("primvokon.db")
}

pub fn secrets_file() -> PathBuf {
    data_dir().join("secrets.json")
}

/// Create the directories if needed.
pub fn ensure_dirs() -> std::io::Result<()> {
    std::fs::create_dir_all(config_dir())?;
    std::fs::create_dir_all(data_dir())?;
    std::fs::create_dir_all(cache_dir())?;
    Ok(())
}
