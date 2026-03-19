fn main() {
    println!("cargo:rerun-if-changed=migrations/sqlite");
    tauri_build::build()
}
