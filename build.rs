fn main() {
    // Declaring the app commands makes every webview need an explicit capability to call them.
    let commands = &["list_services", "add_service", "remove_service", "switch_service", "set_panel", "notify"];
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(commands)))
        .expect("failed to run tauri-build");
}
