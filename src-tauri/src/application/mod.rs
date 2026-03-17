pub mod config;
pub mod dto;
pub mod service;

use tauri::{AppHandle, Manager, Runtime};

use self::config::{load_from_app_data, RuntimeConfigState};
use self::service::{
    bootstrap_not_implemented_service, bootstrap_postgres_service, bootstrap_sqlite_service,
};
use crate::repository::RepositoryLayer;

pub fn bootstrap<R: Runtime>(app: &AppHandle<R>) {
    let config_report = load_from_app_data(app);
    let runtime_mode = config_report.config.runtime_mode.clone();
    let silent_days_threshold = config_report.config.silent_days_threshold;
    tracing::info!(
        component = "config",
        config_path = %config_report.config_path.display(),
        runtime_mode = %config_report.config.runtime_mode,
        warning_count = config_report.warnings.len(),
        used_fallback = config_report.used_fallback,
        "runtime config loaded"
    );

    for warning in &config_report.warnings {
        tracing::warn!(component = "config", warning = %warning, "runtime config warning");
    }

    let repository = RepositoryLayer::new(config_report.config.runtime_mode.clone());
    tracing::info!(
        component = "repository",
        runtime_mode = repository.runtime_mode().as_config_value(),
        "repository layer initialized"
    );

    let service_bootstrap = match runtime_mode {
        config::RuntimeMode::SqliteOnly => {
            tauri::async_runtime::block_on(bootstrap_sqlite_service(app, silent_days_threshold))
                .unwrap_or_else(|error| {
                    panic!("failed to bootstrap sqlite application service: {error}")
                })
        }
        config::RuntimeMode::PostgresOnly => {
            let postgres_dsn =
                resolve_postgres_dsn(&config_report.config).unwrap_or_else(|error| {
                    panic!("{error}");
                });
            tauri::async_runtime::block_on(bootstrap_postgres_service(
                &postgres_dsn,
                silent_days_threshold,
            ))
            .unwrap_or_else(|error| {
                panic!("failed to bootstrap postgres application service: {error}")
            })
        }
        config::RuntimeMode::DualSync => {
            tracing::warn!(
                component = "repository",
                runtime_mode = runtime_mode.as_config_value(),
                "dual_sync backend is not implemented in phase 2, guarded mode enabled"
            );
            bootstrap_not_implemented_service(runtime_mode.as_config_value(), silent_days_threshold)
        }
    };
    for warning in &service_bootstrap.warnings {
        tracing::warn!(
            component = "repository",
            warning = %warning,
            "service bootstrap warning"
        );
    }
    tracing::info!(
        component = "repository",
        backend_target = %service_bootstrap.backend_target,
        "application service initialized"
    );

    app.manage(repository);
    app.manage(service_bootstrap.service_state);
    app.manage(RuntimeConfigState::from(config_report));

    update_main_window_title(app);
    let _ = app.package_info();
}

fn resolve_postgres_dsn(config: &config::AppConfig) -> Result<String, String> {
    let Some(dsn) = config.postgres_dsn.as_ref() else {
        return Err(
            "runtime_mode `postgres_only` requires non-empty `postgres_dsn` in config.toml"
                .to_string(),
        );
    };

    let trimmed = dsn.trim();
    if trimmed.is_empty() {
        return Err(
            "runtime_mode `postgres_only` requires non-empty `postgres_dsn` in config.toml"
                .to_string(),
        );
    }

    Ok(trimmed.to_string())
}

fn update_main_window_title<R: Runtime>(app: &AppHandle<R>) {
    let config_state = app.state::<RuntimeConfigState>();
    let mut title = format!(
        "{} [{}]",
        app.package_info().name,
        config_state.config.runtime_mode.as_config_value()
    );
    if config_state.used_fallback {
        title.push_str(" [CONFIG_FALLBACK]");
        tracing::warn!(
            component = "config",
            config_path = %config_state.config_path.display(),
            warning_count = config_state.warnings.len(),
            "fallback mode in effect"
        );
    }

    if let Some(window) = app.get_webview_window("main") {
        if let Err(error) = window.set_title(&title) {
            tracing::warn!(
                component = "config",
                error = %error,
                "failed to set main window title"
            );
        }
    } else {
        tracing::warn!(
            component = "config",
            "main window not found when setting runtime mode title"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::resolve_postgres_dsn;
    use crate::application::config::{AppConfig, RuntimeMode};

    #[test]
    fn postgres_only_requires_non_empty_dsn() {
        let config = AppConfig {
            runtime_mode: RuntimeMode::PostgresOnly,
            postgres_dsn: None,
            silent_days_threshold: 7,
            hotkey: "Alt+Space".to_string(),
        };

        let error = resolve_postgres_dsn(&config).expect_err("missing dsn should fail");
        assert!(error.contains("postgres_dsn"));
    }

    #[test]
    fn postgres_only_accepts_trimmed_dsn() {
        let config = AppConfig {
            runtime_mode: RuntimeMode::PostgresOnly,
            postgres_dsn: Some("  postgres://user:pass@localhost:5432/remember  ".to_string()),
            silent_days_threshold: 7,
            hotkey: "Alt+Space".to_string(),
        };

        let dsn = resolve_postgres_dsn(&config).expect("dsn should pass");
        assert_eq!(dsn, "postgres://user:pass@localhost:5432/remember");
    }
}
