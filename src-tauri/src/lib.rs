mod adapter;
pub mod application;
pub mod repository;

use std::sync::Once;

use tauri::{AppHandle, Manager};
use tracing_subscriber::{fmt, EnvFilter};
#[cfg(target_os = "windows")]
use window_vibrancy::apply_blur;

static TRACING_INIT: Once = Once::new();

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();
    configure_windows_webview_background();
    tracing::info!(component = "bootstrap", "starting tauri runtime");

    let builder = tauri::Builder::default();
    let builder = adapter::register(builder);

    builder
        .setup(|app| {
            enforce_startup_window_state(&app.handle());
            apply_main_window_blur(&app.handle());
            application::bootstrap(&app.handle());
            adapter::bootstrap_runtime(&app.handle());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn configure_windows_webview_background() {
    #[cfg(target_os = "windows")]
    {
        const WEBVIEW2_BACKGROUND_KEY: &str = "WEBVIEW2_DEFAULT_BACKGROUND_COLOR";
        const TRANSPARENT_RGBA_HEX: &str = "00000000";

        std::env::set_var(WEBVIEW2_BACKGROUND_KEY, TRANSPARENT_RGBA_HEX);
        tracing::info!(
            component = "bootstrap",
            key = WEBVIEW2_BACKGROUND_KEY,
            value = TRANSPARENT_RGBA_HEX,
            "configured webview2 transparent default background"
        );
    }
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

    if matches!(window.is_fullscreen(), Ok(true)) {
        if let Err(error) = window.set_fullscreen(false) {
            tracing::warn!(
                component = "bootstrap",
                ?error,
                "window startup self-heal failed: set_fullscreen(false)"
            );
        }
    }

    if matches!(window.is_maximized(), Ok(false)) {
        if let Err(error) = window.maximize() {
            tracing::warn!(
                component = "bootstrap",
                ?error,
                "window startup self-heal failed: maximize()"
            );
        }
    }

    tracing::info!(
        component = "bootstrap",
        "window startup state validated: maximized transparent undecorated target"
    );
}

fn apply_main_window_blur(app: &AppHandle) {
    #[cfg(target_os = "windows")]
    {
        let window = app
            .get_webview_window("main")
            .or_else(|| app.webview_windows().into_values().next());

        let Some(window) = window else {
            tracing::warn!(
                component = "bootstrap",
                "window blur setup skipped: no webview window found"
            );
            return;
        };

        if let Err(error) = apply_blur(&window, Some((255, 255, 255, 110))) {
            tracing::warn!(
                component = "bootstrap",
                ?error,
                "window blur setup failed: apply_blur"
            );
            return;
        }

        tracing::info!(
            component = "bootstrap",
            "window blur setup completed with light tint"
        );
    }

    #[cfg(not(target_os = "windows"))]
    let _ = app;
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
