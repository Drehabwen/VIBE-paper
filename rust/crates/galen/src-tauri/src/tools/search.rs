use std::fs;

use api::ToolDefinition;
use async_trait::async_trait;
use serde_json::{json, Value};

use super::workspace_path::resolve_workspace_path;
use super::{Tool, ToolContext};

// ---------------------------------------------------------------------------
// search_files
// ---------------------------------------------------------------------------

pub struct SearchFiles;

#[async_trait]
impl Tool for SearchFiles {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "search_files".into(),
            description: Some(
                "Search for files by glob pattern within the workspace \
                 (e.g. '**/*.json', '*.md'). Also supports grep-style content search \
                 with the 'grep' parameter."
                    .into(),
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Glob pattern for file name matching (e.g., '*.json', '**/notes/*.md')"
                    },
                    "grep": {
                        "type": "string",
                        "description": "Optional: text or regex pattern to search for within matching files"
                    },
                    "path": {
                        "type": "string",
                        "description": "Optional subdirectory to search in (defaults to workspace root)"
                    }
                },
                "required": ["pattern"]
            }),
        }
    }

    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<String, String> {
        let pattern = input["pattern"]
            .as_str()
            .ok_or("Missing 'pattern' parameter")?;
        let grep = input["grep"].as_str().filter(|s| !s.is_empty());
        let search_path = input["path"].as_str().unwrap_or("");
        let target = resolve_workspace_path(&ctx.workspace_root, search_path)?;

        let mut results: Vec<String> = Vec::new();
        // Use glob to find matching files
        let glob_pattern = target.join(pattern);
        let glob_str = glob_pattern.to_string_lossy().to_string();
        let paths = glob::glob(&glob_str)
            .map_err(|e| format!("Invalid glob pattern: {e}"))?;
        for entry in paths {
            match entry {
                Ok(p) => {
                    let rel = p
                        .strip_prefix(&target)
                        .unwrap_or(&p)
                        .to_string_lossy()
                        .to_string();
                    let meta = fs::metadata(&p).ok();
                    let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
                    let prefix = if is_dir { "[DIR] " } else { "[FILE]" };
                    if let Some(grep_text) = grep {
                        if !is_dir {
                            if let Ok(content) = fs::read_to_string(&p) {
                                if content.contains(grep_text) {
                                    let preview = content
                                        .lines()
                                        .filter(|l| l.contains(grep_text))
                                        .take(5)
                                        .collect::<Vec<_>>()
                                        .join("\n  ");
                                    results.push(format!(
                                        "{} {} (matches)\n  {}",
                                        prefix, rel, preview
                                    ));
                                }
                            }
                        }
                    } else {
                        let size = meta.map(|m| m.len()).unwrap_or(0);
                        results.push(format!("{} {} ({} bytes)", prefix, rel, size));
                    }
                }
                Err(e) => results.push(format!("Error: {e}")),
            }
        }
        if results.is_empty() {
            Ok(format!(
                "No files found matching '{}' in {}",
                pattern,
                if search_path.is_empty() {
                    "workspace root"
                } else {
                    search_path
                }
            ))
        } else {
            Ok(format!(
                "Search results for '{}':\n{}",
                pattern,
                results.join("\n")
            ))
        }
    }
}
