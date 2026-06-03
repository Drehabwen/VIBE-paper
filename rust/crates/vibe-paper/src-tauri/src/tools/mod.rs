use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use api::ToolDefinition;
use async_trait::async_trait;
use medical_core::MedicalCore;
use serde_json::Value;

use crate::backend::ChatEvent;

pub mod command;
pub mod fs;
pub mod medical;
pub mod search;
pub mod workspace_path;

// ---------------------------------------------------------------------------
// Tool trait & ToolContext
// ---------------------------------------------------------------------------

/// Context passed to every tool during execution.
pub struct ToolContext {
    pub medical: Arc<MedicalCore>,
    pub workspace_root: Mutex<Option<std::path::PathBuf>>,
    event_sender: Option<Arc<dyn Fn(ChatEvent) + Send + Sync>>,
}

impl ToolContext {
    #[allow(dead_code)]
    pub fn new(
        medical: Arc<MedicalCore>,
        workspace_root: Mutex<Option<std::path::PathBuf>>,
    ) -> Self {
        Self {
            medical,
            workspace_root,
            event_sender: None,
        }
    }

    pub fn with_event_sender(
        medical: Arc<MedicalCore>,
        workspace_root: Mutex<Option<std::path::PathBuf>>,
        event_sender: Arc<dyn Fn(ChatEvent) + Send + Sync>,
    ) -> Self {
        Self {
            medical,
            workspace_root,
            event_sender: Some(event_sender),
        }
    }

    pub fn send_event(&self, event: ChatEvent) {
        if let Some(ref sender) = self.event_sender {
            sender(event);
        }
    }
}

/// Each tool must implement this trait: provide a definition and an async execute method.
#[async_trait]
pub trait Tool: Send + Sync {
    fn definition(&self) -> ToolDefinition;
    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<String, String>;
}

// ---------------------------------------------------------------------------
// ToolRegistry
// ---------------------------------------------------------------------------

/// Central registry that owns all tools and dispatches execution by name.
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a single tool.
    pub fn register(&mut self, tool: Box<dyn Tool>) {
        let def = tool.definition();
        self.tools.insert(def.name, tool);
    }

    /// Register multiple tools at once.
    pub fn register_all(&mut self, tools: Vec<Box<dyn Tool>>) {
        for tool in tools {
            self.register(tool);
        }
    }

    /// Register all built-in tools (medical + fs + search + command).
    pub fn register_builtin(&mut self) {
        self.register_all(vec![
            // Medical tools
            Box::new(medical::SearchPubmed),
            Box::new(medical::FetchArticle),
            Box::new(medical::FormatCitation),
            // Filesystem tools
            Box::new(fs::CreateDirectory),
            Box::new(fs::WriteFile),
            Box::new(fs::ReadFile),
            Box::new(fs::ListFiles),
            Box::new(fs::SavePaper),
            Box::new(fs::DeleteFile),
            Box::new(fs::DeleteDirectory),
            Box::new(fs::MoveFile),
            // Search tools
            Box::new(search::SearchFiles),
            // Command tools
            Box::new(command::ExecuteCommand),
        ]);
    }

    /// Return all tool definitions (for sending to the model).
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools
            .values()
            .map(|t| t.definition())
            .collect()
    }

    /// Execute a tool by name, returning success or error as a String.
    pub async fn execute(
        &self,
        name: &str,
        input: Value,
        ctx: &ToolContext,
    ) -> Result<String, String> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| format!("Unknown tool: {name}"))?;
        tool.execute(input, ctx).await
    }

    /// Get tool names (for diagnostics / logging).
    #[allow(dead_code)]
    pub fn tool_names(&self) -> Vec<&str> {
        self.tools.keys().map(|k| k.as_str()).collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        let mut registry = Self::new();
        registry.register_builtin();
        registry
    }
}
