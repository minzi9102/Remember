pub mod config;
pub mod dto;
pub mod service;

use tauri::{AppHandle, Manager, Runtime};

use self::config::{load_from_app_data, RuntimeConfigState};
use self::service::bootstrap_sqlite_service;
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
    if !matches!(runtime_mode, config::RuntimeMode::SqliteOnly) {
        tracing::warn!(
            component = "repository",
            runtime_mode = runtime_mode.as_config_value(),
            "runtime mode backend injection is not enabled yet, sqlite backend will be used in this phase"
        );
    }

    let service_bootstrap =
        tauri::async_runtime::block_on(bootstrap_sqlite_service(app, silent_days_threshold))
            .unwrap_or_else(|error| panic!("failed to bootstrap application service: {error}"));
    for warning in &service_bootstrap.warnings {
        tracing::warn!(
            component = "repository",
            warning = %warning,
            "sqlite bootstrap warning"
        );
    }
    tracing::info!(
        component = "repository",
        sqlite_path = %service_bootstrap.database_path.display(),
        "application service initialized with sqlite backend"
    );

    app.manage(repository);
    app.manage(service_bootstrap.service_state);
    app.manage(RuntimeConfigState::from(config_report));

    update_main_window_title(app);
    let _ = app.package_info();
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
