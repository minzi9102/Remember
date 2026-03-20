mod adapter;
pub mod application;
pub mod repository;

use std::sync::Once;

use tauri::{AppHandle, Manager};
use tracing_subscriber::{fmt, EnvFilter};

static TRACING_INIT: Once = Once::new();

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();
    tracing::info!(component = "bootstrap", "starting tauri runtime");

    let builder = tauri::Builder::default();
    let builder = adapter::register(builder);

    builder
        .setup(|app| {
            enforce_startup_window_state(&app.handle());
            application::bootstrap(&app.handle());
            adapter::bootstrap_runtime(&app.handle());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn enforce_startup_window_state(app: &AppHandle) {
    let window = app
        .get_webview_window("main")
        .or_else(|| app.webview_windows().into_values().next());

    let Some(window) = window else {
        tracing::warn!(
            component = "bootstrap",
            "window startup self-heal skipped: no webview window found"
        );
        return;
    };

    if matches!(window.is_decorated(), Ok(true)) {
        if let Err(error) = window.set_decorations(false) {
            tracing::warn!(
                component = "bootstrap",
                ?error,
                "window startup self-heal failed: set_decorations(false)"
            );
        }
    }

    #[cfg(target_os = "windows")]
    if let Err(error) = window.set_shadow(false) {
        tracing::warn!(
            component = "bootstrap",
            ?error,
            "window startup self-heal failed: set_shadow(false)"
        );
    }

    if matches!(window.is_fullscreen(), Ok(false)) {
        if let Err(error) = window.set_fullscreen(true) {
            tracing::warn!(
                component = "bootstrap",
                ?error,
                "window startup self-heal failed: set_fullscreen(true)"
            );
            return;
        }
    }

    tracing::info!(
        component = "bootstrap",
        "window startup state validated: fullscreen transparent undecorated target"
    );
}

fn init_tracing() {
    TRACING_INIT.call_once(|| {
        let env_filter =
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
        fmt()
            .json()
            .with_env_filter(env_filter)
            .with_target(true)
            .with_current_span(false)
            .with_span_list(false)
            .init();
    });
}
