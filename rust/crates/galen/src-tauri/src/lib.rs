mod backend;
mod commands;
mod tools;
mod workspace;

use commands::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let backend = backend::ChatBackend::new();
    let ws_config = workspace::WorkspaceConfig::load();

    tauri::Builder::default()
        .manage(AppState {
            backend: std::sync::Mutex::new(backend),
            ws_config: std::sync::Mutex::new(ws_config),
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_models,
            commands::get_workspace_root,
            commands::set_workspace,
            commands::list_workspace_files,
            commands::read_workspace_file,
            commands::send_message,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Galen");
}
