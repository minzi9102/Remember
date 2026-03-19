pub mod config;
pub mod dto;
pub mod service;

use tauri::{AppHandle, Manager, Runtime};

use self::config::{load_from_app_data, RuntimeConfigState, SQLITE_RUNTIME_MODE};
use self::service::bootstrap_sqlite_service;
use crate::repository::RepositoryLayer;

pub fn bootstrap<R: Runtime>(app: &AppHandle<R>) {
    let config_report = load_from_app_data(app);
    let silent_days_threshold = config_report.config.silent_days_threshold;
    tracing::info!(
        component = "config",
        config_path = %config_report.config_path.display(),
        runtime_mode = SQLITE_RUNTIME_MODE,
        warning_count = config_report.warnings.len(),
        used_fallback = config_report.used_fallback,
        "runtime config loaded"
    );

    for warning in &config_report.warnings {
        tracing::warn!(component = "config", warning = %warning, "runtime config warning");
    }

    let repository = RepositoryLayer::new();
    tracing::info!(
        component = "repository",
        runtime_mode = repository.runtime_mode(),
        "repository layer initialized"
    );

    let service_bootstrap =
        tauri::async_runtime::block_on(bootstrap_sqlite_service(app, silent_days_threshold))
            .unwrap_or_else(|error| {
                panic!("failed to bootstrap sqlite application service: {error}")
            });
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
    let startup_self_heal = service_bootstrap.service_state.startup_self_heal();
    tracing::info!(
        component = "repository",
        scanned_alerts = startup_self_heal.scanned_alerts,
        repaired_alerts = startup_self_heal.repaired_alerts,
        unresolved_alerts = startup_self_heal.unresolved_alerts,
        failed_alerts = startup_self_heal.failed_alerts,
        completed_at = %startup_self_heal.completed_at,
        "startup self-heal completed"
    );
    for message in &startup_self_heal.messages {
        tracing::warn!(
            component = "repository",
            warning = %message,
            "startup self-heal warning"
        );
    }

    app.manage(repository);
    app.manage(service_bootstrap.service_state);
    app.manage(RuntimeConfigState::from(config_report));

    update_main_window_title(app);
    let _ = app.package_info();
}

fn update_main_window_title<R: Runtime>(app: &AppHandle<R>) {
    let config_state = app.state::<RuntimeConfigState>();
    let mut title = format!("{} [{}]", app.package_info().name, SQLITE_RUNTIME_MODE);
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
