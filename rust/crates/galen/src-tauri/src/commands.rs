use std::sync::Mutex;

use tauri::{Emitter, State, Window};

use crate::backend::{self, ChatBackend, ChatEvent, FileEntry, ModelConfig};
use crate::workspace::WorkspaceConfig;

pub struct AppState {
    pub backend: Mutex<ChatBackend>,
    pub ws_config: Mutex<WorkspaceConfig>,
}

// ---------------------------------------------------------------------------
// Simple query commands (no async work)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_models(state: State<AppState>) -> Vec<ModelConfig> {
    state.backend.lock().unwrap().all_models()
}

#[tauri::command]
pub fn get_workspace_root(state: State<AppState>) -> Option<String> {
    state.backend
        .lock()
        .unwrap()
        .get_workspace_root()
        .map(|p| p.to_string_lossy().to_string())
}

#[tauri::command]
pub fn set_workspace(state: State<AppState>, path: String) -> Result<(), String> {
    let pb = std::path::PathBuf::from(&path);
    if !pb.exists() || !pb.is_dir() {
        return Err("Path does not exist or is not a directory".into());
    }
    let backend = state.backend.lock().unwrap();
    backend.set_workspace_root(Some(pb));
    let mut config = state.ws_config.lock().unwrap();
    config.set_workspace(&path);
    Ok(())
}

#[tauri::command]
pub fn list_workspace_files(
    state: State<AppState>,
    path: Option<String>,
) -> Result<Vec<FileEntry>, String> {
    let backend = state.backend.lock().unwrap();
    let root = backend
        .get_workspace_root()
        .ok_or("No workspace selected")?;
    let target = if let Some(ref sub) = path {
        root.join(sub)
    } else {
        root.clone()
    };

    let mut entries = Vec::new();
    let dir_iter =
        std::fs::read_dir(&target).map_err(|e| format!("Failed to read directory: {e}"))?;
    for entry in dir_iter {
        let entry = entry.map_err(|e| format!("Failed: {e}"))?;
        let name = entry.file_name().to_string_lossy().to_string();
        let meta = entry.metadata().ok();
        let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
        let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
        let ep = entry.path();
        let rel = ep
            .strip_prefix(&target)
            .unwrap_or(&ep)
            .to_string_lossy()
            .to_string();
        entries.push(FileEntry {
            name,
            path: rel,
            is_dir,
            size,
        });
    }
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.cmp(&b.name)));
    Ok(entries)
}

#[tauri::command]
pub fn read_workspace_file(state: State<AppState>, path: String) -> Result<String, String> {
    let backend = state.backend.lock().unwrap();
    let root = backend
        .get_workspace_root()
        .ok_or("No workspace selected")?;
    let target = root.join(&path);
    std::fs::read_to_string(&target).map_err(|e| format!("Failed to read file: {e}"))
}

// ---------------------------------------------------------------------------
// Chat command (async, extracts data from mutex before spawning)
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn send_message(
    state: State<'_, AppState>,
    window: Window,
    message: String,
    model_alias: String,
) -> Result<(), String> {
    // Phase 1: extract all needed data from locked state (before any .await)
    let (model_id, medical, router, workspace_root) = {
        let backend = state.backend.lock().unwrap();
        let model_id = backend.resolve_model(&model_alias);
        let medical = backend.medical.clone();
        let router = backend.router.clone();
        let ws = Mutex::new(backend.workspace_root.lock().unwrap().clone());
        (model_id, medical, router, ws)
    };

    // Phase 2: spawn chat in background, emitting events to the window
    let window_clone = window.clone();
    tokio::spawn(async move {
        let result = backend::run_chat(
            model_alias,
            model_id,
            message,
            Vec::new(), // TODO: multi-turn history
            medical,
            router,
            workspace_root,
            move |event| {
                match &event {
                    ChatEvent::Delta(text) => {
                        let _ = window.emit("chat-delta", text);
                    }
                    ChatEvent::Done(text) => {
                        let _ = window.emit("chat-done", text);
                    }
                    ChatEvent::Error(e) => {
                        let _ = window.emit("chat-error", e);
                    }
                    ChatEvent::SearchResults(papers) => {
                        let _ = window.emit("search-results", papers);
                    }
                    ChatEvent::WorkspaceRoot(path) => {
                        let _ = window.emit("workspace-root", path);
                    }
                    ChatEvent::WorkspaceFileList(files) => {
                        let _ = window.emit("workspace-file-list", files);
                    }
                    ChatEvent::WorkspaceFileContent { path, content } => {
                        let _ = window.emit(
                            "workspace-file-content",
                            serde_json::json!({ "path": path, "content": content }),
                        );
                    }
                }
            },
        )
        .await;

        if let Err(e) = result {
            let _ = window_clone.emit("chat-error", e);
        }
    });

    Ok(())
}
