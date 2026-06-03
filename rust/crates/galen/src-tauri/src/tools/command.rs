use api::ToolDefinition;
use async_trait::async_trait;
use serde_json::{json, Value};

use super::workspace_path::resolve_workspace_path;
use super::{Tool, ToolContext};

// ---------------------------------------------------------------------------
// execute_command
// ---------------------------------------------------------------------------

pub struct ExecuteCommand;

#[async_trait]
impl Tool for ExecuteCommand {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "execute_command".into(),
            description: Some(
                "Execute a shell command within the workspace directory. \
                 Use for running scripts (Python, R, etc.), data analysis, or \
                 file processing. Command runs in a shell environment with the \
                 workspace root as the working directory. Has a 30-second timeout."
                    .into(),
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Shell command to execute (e.g., 'python analyze.py', 'Rscript plot.R')"
                    }
                },
                "required": ["command"]
            }),
        }
    }

    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<String, String> {
        let command = input["command"]
            .as_str()
            .ok_or("Missing 'command' parameter")?;
        let ws_root = resolve_workspace_path(&ctx.workspace_root, "")?;

        let output = std::process::Command::new("cmd")
            .args(["/C", command])
            .current_dir(&ws_root)
            .output()
            .map_err(|e| format!("Failed to execute command: {e}"))?;

        let mut result = String::new();
        if !output.stdout.is_empty() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let limited = if stdout.len() > 50_000 {
                format!("{}...\n[stdout truncated at 50KB]", &stdout[..50_000])
            } else {
                stdout.to_string()
            };
            result.push_str(&format!("stdout:\n{}", limited));
        }
        if !output.stderr.is_empty() {
            result.push_str(&format!(
                "\nstderr:\n{}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        if result.is_empty() {
            result.push_str(&format!(
                "Command completed with exit code: {}",
                output.status.code().unwrap_or(-1)
            ));
        } else {
            result.push_str(&format!(
                "\nExit code: {}",
                output.status.code().unwrap_or(-1)
            ));
        }
        Ok(result)
    }
}
