mod adapter;
mod application;
mod repository;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    let builder = adapter::register(builder);

    builder
        .setup(|app| {
            application::bootstrap(&app.handle());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
