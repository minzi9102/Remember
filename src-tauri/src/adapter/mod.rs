mod hotkey;
mod rpc;

pub fn register<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![rpc::rpc_invoke])
}

pub fn bootstrap_runtime<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    hotkey::bootstrap(app);
}
