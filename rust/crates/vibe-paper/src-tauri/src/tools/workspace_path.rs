use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

/// Resolve a relative path against the workspace root, with sandbox enforcement.
/// Ensures the resolved path does not escape the workspace via symlinks or `..` traversal.
pub fn resolve_workspace_path(
    workspace_root: &Mutex<Option<PathBuf>>,
    rel: &str,
) -> Result<PathBuf, String> {
    let guard = workspace_root
        .lock()
        .map_err(|e| format!("Workspace lock error: {e}"))?;
    let root = guard
        .as_ref()
        .ok_or_else(|| {
            "No workspace selected. Please select a workspace folder first.".to_string()
        })?;
    let resolved = root.join(rel);
    // Ensure the resolved path is still within the workspace root
    let canonical =
        fs::canonicalize(&root).map_err(|e| format!("Cannot resolve workspace root: {e}"))?;
    match fs::canonicalize(&resolved) {
        Ok(p) if p.starts_with(&canonical) => Ok(resolved),
        Ok(_) => Err("Access denied: path is outside workspace".to_string()),
        Err(_) => {
            // If the path doesn't exist yet (e.g., for writes), do a path traversal check
            let root_str = canonical.to_string_lossy().to_string();
            let resolved_str = resolved.to_string_lossy().to_string();
            if resolved_str.starts_with(&root_str) {
                Ok(resolved)
            } else {
                // Attempt simple path traversal check
                let root_parts: Vec<&str> = canonical
                    .components()
                    .map(|c| c.as_os_str().to_str().unwrap_or(""))
                    .collect();
                let mut resolved_parts: Vec<&str> = resolved
                    .components()
                    .map(|c| c.as_os_str().to_str().unwrap_or(""))
                    .collect();
                // Filter out ".." and "."
                resolved_parts.retain(|p| *p != ".." && *p != ".");
                if resolved_parts.starts_with(&root_parts) {
                    Ok(resolved)
                } else {
                    Err("Access denied: path is outside workspace".to_string())
                }
            }
        }
    }
}
