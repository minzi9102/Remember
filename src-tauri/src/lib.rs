mod adapter;
pub mod application;
pub mod repository;

use std::sync::Once;

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
            application::bootstrap(&app.handle());
            adapter::bootstrap_runtime(&app.handle());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
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
