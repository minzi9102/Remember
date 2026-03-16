pub mod config;

use tauri::{AppHandle, Manager, Runtime};

use self::config::{load_from_app_data, RuntimeConfigState};
use crate::repository::RepositoryLayer;

pub fn bootstrap<R: Runtime>(app: &AppHandle<R>) {
    let config_report = load_from_app_data(app);
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
    app.manage(repository);
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
