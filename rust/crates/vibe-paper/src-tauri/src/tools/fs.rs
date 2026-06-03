use std::fs;

use api::ToolDefinition;
use async_trait::async_trait;
use serde_json::{json, Value};

use super::workspace_path::resolve_workspace_path;
use super::{Tool, ToolContext};
use crate::backend::{ChatEvent, FileEntry};

// ---------------------------------------------------------------------------
// create_directory
// ---------------------------------------------------------------------------

pub struct CreateDirectory;

#[async_trait]
impl Tool for CreateDirectory {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "create_directory".into(),
            description: Some(
                "Create a directory in the workspace. Creates all parent directories if needed."
                    .into(),
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Directory path relative to workspace root"
                    }
                },
                "required": ["path"]
            }),
        }
    }

    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<String, String> {
        let path = input["path"]
            .as_str()
            .ok_or("Missing 'path' parameter")?;
        let target = resolve_workspace_path(&ctx.workspace_root, path)?;
        fs::create_dir_all(&target)
            .map_err(|e| format!("Failed to create directory: {e}"))?;
        Ok(format!("Created: {}", target.display()))
    }
}

// ---------------------------------------------------------------------------
// write_file
// ---------------------------------------------------------------------------

pub struct WriteFile;

#[async_trait]
impl Tool for WriteFile {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "write_file".into(),
            description: Some(
                "Write content to a file in the workspace. Creates parent directories if needed."
                    .into(),
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path relative to workspace root"
                    },
                    "content": {
                        "type": "string",
                        "description": "Content to write to the file"
                    }
                },
                "required": ["path", "content"]
            }),
        }
    }

    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<String, String> {
        let path = input["path"]
            .as_str()
            .ok_or("Missing 'path' parameter")?;
        let content = input["content"]
            .as_str()
            .ok_or("Missing 'content' parameter")?;
        let target = resolve_workspace_path(&ctx.workspace_root, path)?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create parent directories: {e}"))?;
        }
        let bytes = content.as_bytes();
        fs::write(&target, bytes).map_err(|e| format!("Failed to write file: {e}"))?;
        Ok(format!(
            "Wrote {} bytes to {}",
            bytes.len(),
            target.display()
        ))
    }
}

// ---------------------------------------------------------------------------
// read_file
// ---------------------------------------------------------------------------

pub struct ReadFile;

#[async_trait]
impl Tool for ReadFile {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "read_file".into(),
            description: Some("Read the contents of a file from the workspace.".into()),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path relative to workspace root"
                    }
                },
                "required": ["path"]
            }),
        }
    }

    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<String, String> {
        let path = input["path"]
            .as_str()
            .ok_or("Missing 'path' parameter")?;
        let target = resolve_workspace_path(&ctx.workspace_root, path)?;
        let content =
            fs::read_to_string(&target).map_err(|e| format!("Failed to read file: {e}"))?;
        // Limit to 100KB
        let limited = if content.len() > 102_400 {
            format!("{}...\n[File truncated at 100KB]", &content[..102_400])
        } else {
            content.clone()
        };
        ctx.send_event(ChatEvent::WorkspaceFileContent {
            path: path.to_string(),
            content: content.clone(),
        });
        Ok(limited)
    }
}

// ---------------------------------------------------------------------------
// list_files
// ---------------------------------------------------------------------------

pub struct ListFiles;

#[async_trait]
impl Tool for ListFiles {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "list_files".into(),
            description: Some(
                "List files and directories in the workspace. If no path is provided, \
                 lists the workspace root directory."
                    .into(),
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Optional directory path relative to workspace root (defaults to root)"
                    }
                },
                "required": []
            }),
        }
    }

    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<String, String> {
        let path = input["path"].as_str().unwrap_or("");
        let target = resolve_workspace_path(&ctx.workspace_root, path)?;
        let mut entries: Vec<FileEntry> = Vec::new();

        let dir_iter =
            fs::read_dir(&target).map_err(|e| format!("Failed to read directory: {e}"))?;
        for entry in dir_iter {
            let entry = entry.map_err(|e| format!("Failed to read entry: {e}"))?;
            let name = entry.file_name().to_string_lossy().to_string();
            let meta = entry.metadata().ok();
            let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
            let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);

            let entry_path = entry.path();
            let rel = entry_path
                .strip_prefix(&target)
                .unwrap_or(&entry_path)
                .to_string_lossy()
                .to_string();

            entries.push(FileEntry {
                name,
                path: if path.is_empty() {
                    rel
                } else {
                    format!("{}/{}", path, rel)
                },
                is_dir,
                size,
            });
        }

        // Sort: dirs first, then by name
        entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.cmp(&b.name)));

        ctx.send_event(ChatEvent::WorkspaceFileList(entries.clone()));

        // Build human-readable listing
        let listing: Vec<String> = entries
            .iter()
            .map(|e| {
                let prefix = if e.is_dir { "[DIR] " } else { "[FILE]" };
                let size_info = if e.is_dir {
                    String::new()
                } else {
                    format!(" ({} bytes)", e.size)
                };
                format!("{} {}{}", prefix, e.path, size_info)
            })
            .collect();
        if listing.is_empty() {
            Ok(format!("Directory is empty: {}", path))
        } else {
            Ok(format!(
                "Contents of {}:\n{}",
                if path.is_empty() {
                    "workspace root"
                } else {
                    path
                },
                listing.join("\n")
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// save_paper
// ---------------------------------------------------------------------------

pub struct SavePaper;

#[async_trait]
impl Tool for SavePaper {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "save_paper".into(),
            description: Some(
                "Save paper metadata as a JSON file to the workspace papers/ directory.".into(),
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pmid": {
                        "type": "string",
                        "description": "PubMed ID of the paper to save"
                    },
                    "format": {
                        "type": "string",
                        "description": "Output format (default: json)",
                        "default": "json"
                    }
                },
                "required": ["pmid"]
            }),
        }
    }

    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<String, String> {
        let pmid = input["pmid"]
            .as_str()
            .ok_or("Missing 'pmid' parameter")?;
        let format = input["format"].as_str().unwrap_or("json");

        // Fetch the paper metadata
        let paper = ctx
            .medical
            .fetch_article(pmid)
            .await
            .map_err(|e| format!("PubMed fetch error: {e}"))?
            .ok_or_else(|| format!("No article found for PMID: {pmid}"))?;

        // Ensure papers/ directory exists
        let papers_dir = resolve_workspace_path(&ctx.workspace_root, "papers")?;
        fs::create_dir_all(&papers_dir)
            .map_err(|e| format!("Failed to create papers directory: {e}"))?;

        let filename = format!("{}.{}", pmid, format);
        let target = papers_dir.join(&filename);

        let content = match format {
            "json" => {
                serde_json::to_string_pretty(&paper)
                    .map_err(|e| format!("Failed to serialize paper: {e}"))?
            }
            _ => return Err(format!("Unsupported format: {format}. Use 'json'.")),
        };

        fs::write(&target, content)
            .map_err(|e| format!("Failed to write paper file: {e}"))?;

        Ok(format!("Saved paper {} to papers/{}", pmid, filename))
    }
}

// ---------------------------------------------------------------------------
// delete_file
// ---------------------------------------------------------------------------

pub struct DeleteFile;

#[async_trait]
impl Tool for DeleteFile {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "delete_file".into(),
            description: Some(
                "Delete a file from the workspace. Cannot delete directories \
                 (use delete_directory for that). Operation is irreversible."
                    .into(),
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path relative to workspace root"
                    }
                },
                "required": ["path"]
            }),
        }
    }

    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<String, String> {
        let path = input["path"]
            .as_str()
            .ok_or("Missing 'path' parameter")?;
        let target = resolve_workspace_path(&ctx.workspace_root, path)?;
        let meta = fs::metadata(&target).map_err(|e| format!("Cannot access file: {e}"))?;
        if meta.is_dir() {
            return Err("Use delete_directory for directories, not delete_file.".into());
        }
        fs::remove_file(&target).map_err(|e| format!("Failed to delete file: {e}"))?;
        Ok(format!("Deleted file: {}", target.display()))
    }
}

// ---------------------------------------------------------------------------
// delete_directory
// ---------------------------------------------------------------------------

pub struct DeleteDirectory;

#[async_trait]
impl Tool for DeleteDirectory {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "delete_directory".into(),
            description: Some(
                "Recursively delete a directory and all its contents from the workspace. \
                 Operation is irreversible. Use with caution."
                    .into(),
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Directory path relative to workspace root"
                    }
                },
                "required": ["path"]
            }),
        }
    }

    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<String, String> {
        let path = input["path"]
            .as_str()
            .ok_or("Missing 'path' parameter")?;
        let target = resolve_workspace_path(&ctx.workspace_root, path)?;
        let meta = fs::metadata(&target).map_err(|e| format!("Cannot access directory: {e}"))?;
        if !meta.is_dir() {
            return Err("Path is not a directory. Use delete_file for files.".into());
        }
        fs::remove_dir_all(&target).map_err(|e| format!("Failed to delete directory: {e}"))?;
        Ok(format!("Deleted directory: {}", target.display()))
    }
}

// ---------------------------------------------------------------------------
// move_file
// ---------------------------------------------------------------------------

pub struct MoveFile;

#[async_trait]
impl Tool for MoveFile {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "move_file".into(),
            description: Some(
                "Move or rename a file or directory within the workspace. \
                 Creates parent directories if needed."
                    .into(),
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "from": {
                        "type": "string",
                        "description": "Source path relative to workspace root"
                    },
                    "to": {
                        "type": "string",
                        "description": "Destination path relative to workspace root"
                    }
                },
                "required": ["from", "to"]
            }),
        }
    }

    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<String, String> {
        let from = input["from"]
            .as_str()
            .ok_or("Missing 'from' parameter")?;
        let to = input["to"]
            .as_str()
            .ok_or("Missing 'to' parameter")?;
        let source = resolve_workspace_path(&ctx.workspace_root, from)?;
        let dest = resolve_workspace_path(&ctx.workspace_root, to)?;
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create parent directories: {e}"))?;
        }
        fs::rename(&source, &dest).map_err(|e| format!("Failed to move/rename: {e}"))?;
        Ok(format!("Moved {} -> {}", source.display(), dest.display()))
    }
}
