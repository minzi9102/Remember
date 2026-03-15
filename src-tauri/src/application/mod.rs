use tauri::{AppHandle, Runtime};

use crate::repository::RepositoryLayer;

pub fn bootstrap<R: Runtime>(app: &AppHandle<R>) {
    let _repository = RepositoryLayer::new();
    let _ = app.package_info();
}
