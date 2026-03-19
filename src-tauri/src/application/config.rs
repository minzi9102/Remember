use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use tauri::{AppHandle, Manager, Runtime};

const CONFIG_FILE_NAME: &str = "config.toml";
const DEFAULT_HOTKEY: &str = "Alt+Space";
const DEFAULT_SILENT_DAYS_THRESHOLD: u32 = 7;
pub(crate) const APP_DATA_DIR_OVERRIDE_ENV: &str = "REMEMBER_APPDATA_DIR";
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
    pub warnings: Vec<String>,
    pub used_fallback: bool,
}

impl From<ConfigLoadReport> for RuntimeConfigState {
    fn from(report: ConfigLoadReport) -> Self {
        Self {
            config: report.config,
            config_path: report.config_path,
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

pub fn load_from_app_data<R: Runtime>(app: &AppHandle<R>) -> ConfigLoadReport {
    let (config_path, mut warnings) = resolve_config_path(app);
    let mut report = load_from_path(&config_path);
    if !warnings.is_empty() {
        report.used_fallback = true;
        warnings.append(&mut report.warnings);
        report.warnings = warnings;
    }
    report
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

fn resolve_config_path<R: Runtime>(app: &AppHandle<R>) -> (PathBuf, Vec<String>) {
    let mut warnings = Vec::new();

    if let Some(mut override_dir) = resolve_app_data_dir_override() {
        match fs::create_dir_all(&override_dir) {
            Ok(()) => {
                override_dir.push(CONFIG_FILE_NAME);
                return (override_dir, warnings);
            }
            Err(error) => warnings.push(format!(
                "failed to create override app data directory from {APP_DATA_DIR_OVERRIDE_ENV}={}, fallback to platform app data directory: {error}",
                override_dir.display()
            )),
        }
    }

    match app.path().app_data_dir() {
        Ok(mut dir) => {
            dir.push(CONFIG_FILE_NAME);
            (dir, warnings)
        }
        Err(error) => {
            warnings.push(format!(
                "failed to resolve app data directory, fallback path is {CONFIG_FILE_NAME}: {error}"
            ));
            (PathBuf::from(CONFIG_FILE_NAME), warnings)
        }
    }
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

    if let Some(dsn) = parsed.postgres_dsn.as_deref() {
        let trimmed = dsn.trim();
        if !trimmed.is_empty() {
            warnings.push(format!(
                "legacy postgres_dsn is ignored; {SQLITE_RUNTIME_MODE} is always active"
            ));
        }
    }

    let config = AppConfig {
        silent_days_threshold: parsed
            .silent_days_threshold
            .unwrap_or(DEFAULT_SILENT_DAYS_THRESHOLD),
        hotkey,
    };

    Ok((config, warnings, used_fallback))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{load_from_path, resolve_app_data_dir_override, APP_DATA_DIR_OVERRIDE_ENV};

    #[test]
    fn legacy_runtime_modes_are_accepted_but_ignored() {
        let test_cases = ["sqlite_only", "postgres_only", "dual_sync"];

        for raw_mode in test_cases {
            let file_path = create_temp_config_path(raw_mode);
            std::fs::create_dir_all(
                file_path
                    .parent()
                    .expect("temp config path should have a parent"),
            )
            .expect("failed to create temp directory");
            std::fs::write(&file_path, format!("runtime_mode = \"{raw_mode}\""))
                .expect("failed to write temp config");

            let report = load_from_path(&file_path);
            assert_eq!(report.config.silent_days_threshold, 7);
            assert_eq!(report.config.hotkey, "Alt+Space");
            assert!(!report.used_fallback);
            assert_eq!(report.warnings.len(), 1);
            assert!(report.warnings[0].contains("legacy runtime_mode"));

            cleanup_temp_path(&file_path);
        }
    }

    #[test]
    fn warns_when_legacy_postgres_dsn_is_present() {
        let file_path = create_temp_config_path("legacy-postgres-dsn");
        std::fs::create_dir_all(
            file_path
                .parent()
                .expect("temp config path should have a parent"),
        )
        .expect("failed to create temp directory");
        std::fs::write(
            &file_path,
            "runtime_mode = \"dual_sync\"\npostgres_dsn = \"postgres://user:pass@localhost:5432/remember\"\n",
        )
        .expect("failed to write temp config");

        let report = load_from_path(&file_path);

        assert_eq!(report.config.silent_days_threshold, 7);
        assert_eq!(report.config.hotkey, "Alt+Space");
        assert!(!report.used_fallback);
        assert_eq!(report.warnings.len(), 2);
        assert!(report.warnings[0].contains("legacy runtime_mode"));
        assert!(report.warnings[1].contains("legacy postgres_dsn"));

        cleanup_temp_path(&file_path);
    }

    #[test]
    fn ignores_invalid_legacy_runtime_mode_values() {
        let file_path = create_temp_config_path("invalid-runtime-mode");
        std::fs::create_dir_all(
            file_path
                .parent()
                .expect("temp config path should have a parent"),
        )
        .expect("failed to create temp directory");
        std::fs::write(
            &file_path,
            "runtime_mode = \"invalid_mode\"\nhotkey = \"Ctrl+Shift+R\"",
        )
        .expect("failed to write temp config");

        let report = load_from_path(&file_path);
        assert_eq!(report.config.hotkey, "Ctrl+Shift+R");
        assert!(!report.used_fallback);
        assert_eq!(report.warnings.len(), 1);
        assert!(report.warnings[0].contains("legacy runtime_mode"));

        cleanup_temp_path(&file_path);
    }

    #[test]
    fn falls_back_when_config_file_is_missing() {
        let file_path = create_temp_config_path("missing");
        let report = load_from_path(&file_path);

        assert_eq!(report.config.silent_days_threshold, 7);
        assert_eq!(report.config.hotkey, "Alt+Space");
        assert!(report.used_fallback);
        assert!(!report.warnings.is_empty());
        assert!(report.warnings[0].contains("config file not found"));
    }

    #[test]
    fn falls_back_when_hotkey_is_empty() {
        let file_path = create_temp_config_path("empty-hotkey");
        std::fs::create_dir_all(
            file_path
                .parent()
                .expect("temp config path should have a parent"),
        )
        .expect("failed to create temp directory");
        std::fs::write(&file_path, "hotkey = \"   \"").expect("failed to write temp config");

        let report = load_from_path(&file_path);

        assert_eq!(report.config.hotkey, "Alt+Space");
        assert!(report.used_fallback);
        assert_eq!(report.warnings.len(), 1);
        assert!(report.warnings[0].contains("empty hotkey"));

        cleanup_temp_path(&file_path);
    }

    #[test]
    fn reads_app_data_override_from_environment() {
        let override_dir = std::env::temp_dir().join("remember-config-override");
        std::env::set_var(APP_DATA_DIR_OVERRIDE_ENV, &override_dir);

        let resolved = resolve_app_data_dir_override();

        std::env::remove_var(APP_DATA_DIR_OVERRIDE_ENV);
        assert_eq!(resolved, Some(override_dir));
    }

    fn create_temp_config_path(suffix: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock went backwards")
            .as_nanos();
        std::env::temp_dir()
            .join(format!("remember-p1-t2-{suffix}-{nonce}"))
            .join("config.toml")
    }

    fn cleanup_temp_path(path: &PathBuf) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}
