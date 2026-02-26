use std::fs;
use std::path::PathBuf;

use crate::config::{atomic_write, delete_file, get_home_dir};
use crate::error::AppError;

/// Get Antigravity configuration directory.
///
/// Priority:
/// 1. User override from settings (`antigravityConfigDir`)
/// 2. Windows default: `%APPDATA%\Antigravity\User\globalStorage`
/// 3. Fallback: `~/.config/Antigravity/User/globalStorage`
pub fn get_antigravity_dir() -> PathBuf {
    if let Some(custom) = crate::settings::get_antigravity_override_dir() {
        return custom;
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            let trimmed = appdata.trim();
            if !trimmed.is_empty() {
                return PathBuf::from(trimmed)
                    .join("Antigravity")
                    .join("User")
                    .join("globalStorage");
            }
        }
    }

    get_home_dir()
        .join(".config")
        .join("Antigravity")
        .join("User")
        .join("globalStorage")
}

/// Path to Antigravity state database file.
pub fn get_antigravity_state_db_path() -> PathBuf {
    get_antigravity_dir().join("state.vscdb")
}

/// Read Antigravity `state.vscdb` bytes from live directory.
pub fn read_antigravity_state_db_bytes() -> Result<Vec<u8>, AppError> {
    let path = get_antigravity_state_db_path();
    if !path.exists() {
        return Err(AppError::localized(
            "antigravity.state.missing",
            "Antigravity 配置文件不存在：state.vscdb",
            "Antigravity configuration file missing: state.vscdb",
        ));
    }

    fs::read(&path).map_err(|e| AppError::io(&path, e))
}

/// Atomically write Antigravity `state.vscdb` bytes to live directory.
pub fn write_antigravity_state_db_bytes(bytes: &[u8]) -> Result<(), AppError> {
    let path = get_antigravity_state_db_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| AppError::io(parent, e))?;
    }
    atomic_write(&path, bytes)
}

/// Remove Antigravity `state.vscdb` from live directory.
pub fn delete_antigravity_state_db() -> Result<(), AppError> {
    delete_file(&get_antigravity_state_db_path())
}
