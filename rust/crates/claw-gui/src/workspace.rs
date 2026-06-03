use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    pub workspace_root: Option<String>,
    pub recent_workspaces: Vec<String>,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            workspace_root: None,
            recent_workspaces: Vec::new(),
        }
    }
}

impl WorkspaceConfig {
    fn config_path() -> Option<PathBuf> {
        let dir = dirs::config_dir()?.join("galen");
        Some(dir.join("workspace.json"))
    }

    pub fn load() -> Self {
        let path = match Self::config_path() {
            Some(p) => p,
            None => return Self::default(),
        };
        if !path.exists() {
            return Self::default();
        }
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let path = match Self::config_path() {
            Some(p) => p,
            None => return,
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(&path, json);
        }
    }

    pub fn set_workspace(&mut self, path: &str) {
        self.workspace_root = Some(path.to_string());
        // Move to front of recent
        self.recent_workspaces.retain(|p| p != path);
        self.recent_workspaces.insert(0, path.to_string());
        self.recent_workspaces.truncate(5);
        self.save();
    }
}
