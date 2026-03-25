use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::Deserialize;

const CONFIG_FILE_NAME: &str = "config.toml";
const SQLITE_DB_FILE_NAME: &str = "remember.sqlite3";
const DEFAULT_HOTKEY: &str = "Alt+Space";
const DEFAULT_SILENT_DAYS_THRESHOLD: u32 = 7;
pub const APP_DATA_DIR_OVERRIDE_ENV: &str = "REMEMBER_APPDATA_DIR";
pub const SQLITE_RUNTIME_MODE: &str = "sqlite_only";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub silent_days_threshold: u32,
    pub hotkey: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            silent_days_threshold: DEFAULT_SILENT_DAYS_THRESHOLD,
            hotkey: DEFAULT_HOTKEY.to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConfigLoadReport {
    pub config: AppConfig,
    pub config_path: PathBuf,
    pub warnings: Vec<String>,
    pub used_fallback: bool,
}

#[derive(Debug, Clone)]
pub struct RuntimeConfigState {
    pub config: AppConfig,
    pub config_path: PathBuf,
    pub database_path: PathBuf,
    pub warnings: Vec<String>,
    pub used_fallback: bool,
}

impl RuntimeConfigState {
    pub fn load() -> Self {
        let (config_path, mut path_warnings, path_used_fallback) = resolve_config_path();
        let mut report = load_from_path(&config_path);
        if !path_warnings.is_empty() {
            report.warnings.append(&mut path_warnings);
        }
        if path_used_fallback {
            report.used_fallback = true;
        }

        let (database_path, db_warnings, db_used_fallback) = resolve_sqlite_database_path();
        report.warnings.extend(db_warnings);
        if db_used_fallback {
            report.used_fallback = true;
        }

        Self {
            config: report.config,
            config_path: report.config_path,
            database_path,
            warnings: report.warnings,
            used_fallback: report.used_fallback,
        }
    }
}

#[derive(Debug, Deserialize)]
struct RawAppConfig {
    runtime_mode: Option<String>,
    postgres_dsn: Option<String>,
    silent_days_threshold: Option<u32>,
    hotkey: Option<String>,
}

pub fn load_from_path(path: &Path) -> ConfigLoadReport {
    let mut warnings = Vec::new();
    let mut used_fallback = false;
    let config = match fs::read_to_string(path) {
        Ok(raw) => match parse_raw_config(&raw) {
            Ok((parsed, parse_warnings, parse_used_fallback)) => {
                if !parse_warnings.is_empty() {
                    warnings.extend(parse_warnings);
                }
                if parse_used_fallback {
                    used_fallback = true;
                }
                parsed
            }
            Err(error) => {
                warnings.push(format!(
                    "failed to parse config file {}, fallback to defaults: {error}",
                    path.display()
                ));
                used_fallback = true;
                AppConfig::default()
            }
        },
        Err(error) => {
            if error.kind() == ErrorKind::NotFound {
                warnings.push(format!(
                    "config file not found at {}, fallback to defaults",
                    path.display()
                ));
            } else {
                warnings.push(format!(
                    "failed to read config file {}, fallback to defaults: {error}",
                    path.display()
                ));
            }
            used_fallback = true;
            AppConfig::default()
        }
    };

    ConfigLoadReport {
        config,
        config_path: path.to_path_buf(),
        warnings,
        used_fallback,
    }
}

pub fn resolve_config_path() -> (PathBuf, Vec<String>, bool) {
    let (base_dir, warnings, used_fallback) = resolve_data_dir();
    (base_dir.join(CONFIG_FILE_NAME), warnings, used_fallback)
}

pub fn resolve_sqlite_database_path() -> (PathBuf, Vec<String>, bool) {
    let (base_dir, warnings, used_fallback) = resolve_data_dir();
    (base_dir.join(SQLITE_DB_FILE_NAME), warnings, used_fallback)
}

fn resolve_data_dir() -> (PathBuf, Vec<String>, bool) {
    let mut warnings = Vec::new();
    let mut used_fallback = false;

    if let Some(override_dir) = resolve_app_data_dir_override() {
        if let Err(error) = fs::create_dir_all(&override_dir) {
            warnings.push(format!(
                "failed to create override app data directory from {APP_DATA_DIR_OVERRIDE_ENV}={}, fallback to platform app data directory: {error}",
                override_dir.display()
            ));
            used_fallback = true;
        } else {
            return (override_dir, warnings, used_fallback);
        }
    }

    if let Some(platform_dir) = dirs::data_local_dir().or_else(dirs::data_dir) {
        let remember_dir = platform_dir.join("Remember");
        if let Err(error) = fs::create_dir_all(&remember_dir) {
            warnings.push(format!(
                "failed to create platform app data directory {}, fallback to current working directory: {error}",
                remember_dir.display()
            ));
            used_fallback = true;
        } else {
            return (remember_dir, warnings, used_fallback);
        }
    } else {
        warnings.push(
            "failed to resolve platform app data directory, fallback to current working directory"
                .to_string(),
        );
        used_fallback = true;
    }

    (PathBuf::from("."), warnings, used_fallback)
}

fn resolve_app_data_dir_override() -> Option<PathBuf> {
    std::env::var(APP_DATA_DIR_OVERRIDE_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn parse_raw_config(raw: &str) -> Result<(AppConfig, Vec<String>, bool), toml::de::Error> {
    let parsed: RawAppConfig = toml::from_str(raw)?;
    let mut warnings = Vec::new();
    let mut used_fallback = false;

    if let Some(raw_mode) = parsed.runtime_mode.as_deref() {
        let trimmed = raw_mode.trim();
        if !trimmed.is_empty() {
            warnings.push(format!(
                "legacy runtime_mode `{trimmed}` is ignored; {SQLITE_RUNTIME_MODE} is always active"
            ));
        }
    }

    if let Some(dsn) = parsed.postgres_dsn.as_deref() {
        let trimmed = dsn.trim();
        if !trimmed.is_empty() {
            warnings.push(format!(
                "legacy postgres_dsn is ignored; {SQLITE_RUNTIME_MODE} is always active"
            ));
        }
    }

    let hotkey = match parsed.hotkey {
        Some(hotkey) if !hotkey.trim().is_empty() => hotkey,
        Some(_) => {
            warnings.push(format!(
                "empty hotkey, fallback to default `{DEFAULT_HOTKEY}`"
            ));
            used_fallback = true;
            DEFAULT_HOTKEY.to_string()
        }
        None => DEFAULT_HOTKEY.to_string(),
    };

    let config = AppConfig {
        silent_days_threshold: parsed
            .silent_days_threshold
            .unwrap_or(DEFAULT_SILENT_DAYS_THRESHOLD),
        hotkey,
    };

    Ok((config, warnings, used_fallback))
}
