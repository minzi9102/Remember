pub mod config;

use tauri::{AppHandle, Manager, Runtime};

use self::config::{load_from_app_data, RuntimeConfigState};
use crate::repository::RepositoryLayer;

pub fn bootstrap<R: Runtime>(app: &AppHandle<R>) {
    let config_report = load_from_app_data(app);
    println!(
        "[remember][config] loaded from={} runtime_mode={} warnings={}",
        config_report.config_path.display(),
        config_report.config.runtime_mode,
        config_report.warnings.len()
    );

    for warning in &config_report.warnings {
        eprintln!("[remember][config][warning] {warning}");
    }

    let repository = RepositoryLayer::new(config_report.config.runtime_mode.clone());
    println!(
        "[remember][repository] initialized runtime_mode={}",
        repository.runtime_mode().as_config_value()
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
        eprintln!(
            "[remember][config][warning] fallback mode in effect path={} warnings={}",
            config_state.config_path.display(),
            config_state.warnings.len()
        );
    }

    if let Some(window) = app.get_webview_window("main") {
        if let Err(error) = window.set_title(&title) {
            eprintln!("[remember][config][warning] failed to set window title: {error}");
        }
    } else {
        eprintln!("[remember][config][warning] main window not found when setting mode title");
    }
}
