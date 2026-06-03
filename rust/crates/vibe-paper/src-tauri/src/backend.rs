use std::sync::{Arc, Mutex};

use api::{
    ContentBlockDelta, InputContentBlock, InputMessage, MessageRequest, OpenAiCompatClient,
    OpenAiCompatConfig, OutputContentBlock, ProviderClient, StreamEvent as ApiStreamEvent,
    ToolChoice, ToolResultContentBlock,
};
use medical_core::MedicalCore;
use medical_core::types::Paper;
use model_router::ModelRouter;
use std::path::PathBuf;

use crate::tools::{ToolContext, ToolRegistry};

const KNOWN_MODELS: &[(&str, &str)] = &[
    ("opus", "claude-opus-4-6"),
    ("sonnet", "claude-sonnet-4-6"),
    ("haiku", "claude-haiku-4-5-20251001"),
    ("gpt-4o", "gpt-4o"),
    ("gpt-4.1", "gpt-4.1"),
];

const MEDICAL_SYSTEM_PROMPT: &str = "\
你是 VIBE Paper，一个医学科研助手。你的任务是通过对话帮助医学研究人员完成文献检索、\
论文理解和学术写作。\n\
\n\
## 工具使用规则\n\
1. 当用户提到任何医学术语、疾病、药物、基因时，立刻调用 search_pubmed 检索相关文献。\
2. 当用户询问某个术语的含义时，调用 lookup_mesh 查询 MeSH 词表。\
3. 当用户粘贴 PMID 或要求查看某篇论文详情时，调用 fetch_article 获取完整信息。\
4. 当用户要求格式化引用时，调用 format_citation。\
5. 当用户要求保存论文、写笔记、导出引用时，使用 write_file / save_paper 工具保存到工作区。\
6. 当用户要求查看工作区文件时，使用 list_files / read_file 工具。\
7. 当用户要求删除文件/目录时，使用 delete_file / delete_directory 工具。\
8. 当用户要求移动、重命名文件时，使用 move_file 工具。\
9. 当用户要求搜索文件（按名称或内容）时，使用 search_files 工具。\
10. 当用户要求运行脚本、代码或命令（Python、R、数据分析等）时，使用 execute_command 工具。\n\
\n\
## 回答风格\n\
- 检索结果要按相关性整理，标注 PMID、作者、期刊、年份。\
- 解释术语时用医学生能理解的语言，给出临床相关性。\
- 引用格式化后给出可以直接复制使用的文本。";

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
pub struct ModelConfig {
    pub name: String,
    #[allow(dead_code)]
    pub model_id: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String, // relative to workspace root
    pub is_dir: bool,
    pub size: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub enum ChatEvent {
    Delta(String),
    Done(String),
    Error(String),
    SearchResults(Vec<Paper>),
    #[allow(dead_code)]
    WorkspaceRoot(String),
    WorkspaceFileList(Vec<FileEntry>),
    WorkspaceFileContent { path: String, content: String },
}

pub struct ChatBackend {
    pub router: ModelRouter,
    pub medical: Arc<MedicalCore>,
    pub workspace_root: Mutex<Option<PathBuf>>,
}

struct PendingToolCall {
    id: String,
    name: String,
    input_json: String,
}

// ---------------------------------------------------------------------------
// ChatBackend methods
// ---------------------------------------------------------------------------

impl ChatBackend {
    pub fn new() -> Self {
        let router = ModelRouter::load().unwrap_or_else(|e| {
            eprintln!("Failed to load models.toml: {e}, using defaults");
            ModelRouter::default()
        });
        let medical = Arc::new(MedicalCore::new(None));
        Self { router, medical, workspace_root: Mutex::new(None) }
    }

    #[allow(dead_code)]
    pub fn default_model(&self) -> Option<ModelConfig> {
        if let Some(m) = self.router.default_model() {
            Some(ModelConfig {
                name: self.router.resolve_model_id(&m.model_id),
                model_id: m.model_id.clone(),
            })
        } else {
            KNOWN_MODELS.first().map(|(name, id)| ModelConfig {
                name: (*name).to_string(),
                model_id: (*id).to_string(),
            })
        }
    }

    pub fn all_models(&self) -> Vec<ModelConfig> {
        let configured = self.router.all_models();
        if configured.is_empty() {
            KNOWN_MODELS
                .iter()
                .map(|(name, id)| ModelConfig {
                    name: (*name).to_string(),
                    model_id: (*id).to_string(),
                })
                .collect()
        } else {
            configured
                .iter()
                .map(|(alias, entry)| ModelConfig {
                    name: alias.clone(),
                    model_id: entry.model_id.clone(),
                })
                .collect()
        }
    }

    pub fn resolve_model(&self, alias: &str) -> String {
        let resolved = self.router.resolve_model_id(alias);
        if resolved == alias {
            KNOWN_MODELS
                .iter()
                .find(|(name, _)| *name == alias)
                .map(|(_, id)| (*id).to_string())
                .unwrap_or_else(|| resolved)
        } else {
            resolved
        }
    }

    /// Set workspace root (called from UI thread)
    pub fn set_workspace_root(&self, root: Option<PathBuf>) {
        if let Ok(mut guard) = self.workspace_root.lock() {
            *guard = root;
        }
    }

    /// Get current workspace root
    pub fn get_workspace_root(&self) -> Option<PathBuf> {
        self.workspace_root.lock().ok()?.clone()
    }

    /// Send a workspace event from UI thread
    #[allow(dead_code)]
    pub fn send_workspace_event(&self, _path: String) -> bool {
        // This needs to send a ChatEvent to the UI...
        // Since we're on the UI thread, we can't directly send.
        // Instead, this will be handled differently - the UI reads workspace_root
        // from the backend and sends the event itself.
        //
        // For now, just return true - the UI will handle sending events
        true
    }
}

// ---------------------------------------------------------------------------
// Standalone functions (ported from ChatBackend methods)
// ---------------------------------------------------------------------------

fn make_client(
    model_alias: &str,
    model_id: &str,
    router: &ModelRouter,
) -> Result<ProviderClient, Box<dyn std::error::Error + Send>> {
    // Try model-router config first (for models.toml entries)
    if let Some(provider_config) = router.to_provider_config(model_alias) {
        if let Some(api_key) = provider_config.api_key() {
            let base_url: &'static str = Box::leak(
                provider_config
                    .base_url
                    .clone()
                    .unwrap_or_else(|| "https://api.openai.com/v1".into())
                    .into_boxed_str(),
            );
            let config = OpenAiCompatConfig {
                provider_name: Box::leak(
                    provider_config.provider.clone().into_boxed_str(),
                ),
                api_key_env: "",
                base_url_env: "",
                default_base_url: base_url,
                max_request_body_bytes: 104_857_600,
            };
            return Ok(ProviderClient::OpenAi(OpenAiCompatClient::new(
                api_key, config,
            )));
        }
    }
    // Fall back to built-in model resolution
    ProviderClient::from_model(model_id)
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)
}

// ---------------------------------------------------------------------------
// Main chat loop
// ---------------------------------------------------------------------------

pub async fn run_chat<F: Fn(ChatEvent) + Send + Sync + 'static>(
    model_alias: String,
    model_id: String,
    user_message: String,
    history: Vec<InputMessage>,
    medical: Arc<MedicalCore>,
    router: ModelRouter,
    workspace_root: Mutex<Option<PathBuf>>,
    on_event: F,
) -> Result<(), String> {
    let client = make_client(&model_alias, &model_id, &router)
        .map_err(|e| format!("Failed to create client: {e}"))?;

    let mut history = history; // mutable copy
    history.push(InputMessage {
        role: "user".to_string(),
        content: vec![InputContentBlock::Text {
            text: user_message,
        }],
    });

    // Build tool registry and shared context
    let registry = ToolRegistry::default();
    let on_event: Arc<dyn Fn(ChatEvent) + Send + Sync> = Arc::new(on_event);
    let ctx = ToolContext::with_event_sender(medical.clone(), workspace_root, on_event.clone());

    // Multi-turn loop: keep going until model responds with text (no tool calls)
    let mut turn = 0;
    let max_turns = 5;
    loop {
        turn += 1;
        if turn > max_turns {
            on_event(ChatEvent::Error("Reached max tool-call turns".into()));
            break;
        }

        let tools = registry.definitions();

        let request = MessageRequest {
            model: model_id.clone(),
            messages: history.clone(),
            max_tokens: 4096,
            system: Some(MEDICAL_SYSTEM_PROMPT.to_string()),
            tools: Some(tools.clone()),
            tool_choice: Some(ToolChoice::Auto),
            stream: true,
            ..Default::default()
        };

        let mut stream = client
            .stream_message(&request)
            .await
            .map_err(|e| format!("API error: {e}"))?;

        // Collect content blocks for this response
        let mut text_blocks: Vec<String> = Vec::new();
        let mut tool_calls: Vec<PendingToolCall> = Vec::new();
        let mut current_tool: Option<PendingToolCall> = None;
        let mut current_text: String = String::new();
        let mut current_thinking: String = String::new();

        loop {
            match stream.next_event().await {
                Ok(Some(ApiStreamEvent::ContentBlockStart(event))) => {
                    match event.content_block {
                        OutputContentBlock::Text { text } => {
                            // Only initialize the accumulator — don't emit.
                            // ContentBlockDelta events carry the actual streamed text.
                            current_text = text;
                        }
                        OutputContentBlock::ToolUse { id, name, .. } => {
                            // Flush any text/thinking before starting tool call
                            if !current_text.is_empty() {
                                text_blocks.push(std::mem::take(&mut current_text));
                            }
                            current_tool = Some(PendingToolCall {
                                id,
                                name,
                                input_json: String::new(),
                            });
                        }
                        OutputContentBlock::Thinking { thinking, .. } => {
                            current_thinking = thinking;
                        }
                        _ => {}
                    }
                }
                Ok(Some(ApiStreamEvent::ContentBlockDelta(event))) => {
                    match event.delta {
                        ContentBlockDelta::TextDelta { text } => {
                            current_text.push_str(&text);
                            on_event(ChatEvent::Delta(text));
                        }
                        ContentBlockDelta::InputJsonDelta { partial_json } => {
                            if let Some(ref mut tool) = current_tool {
                                tool.input_json.push_str(&partial_json);
                            }
                        }
                        ContentBlockDelta::ThinkingDelta { thinking } => {
                            current_thinking.push_str(&thinking);
                        }
                        _ => {}
                    }
                }
                Ok(Some(ApiStreamEvent::ContentBlockStop(_))) => {
                    // Finish current tool call
                    if let Some(tool) = current_tool.take() {
                        if !tool.input_json.is_empty() {
                            tool_calls.push(tool);
                        }
                    }
                }
                Ok(Some(ApiStreamEvent::MessageStop(_))) => {
                    if !current_text.is_empty() {
                        text_blocks.push(std::mem::take(&mut current_text));
                    }
                    break;
                }
                Ok(None) => break,
                Err(e) => {
                    on_event(ChatEvent::Error(format!("stream error: {e}")));
                    return Ok(());
                }
                _ => {}
            }
        }

        // Build assistant message from collected blocks
        let mut assistant_content: Vec<InputContentBlock> = Vec::new();
        // Reasoning / chain-of-thought must be included so it can be
        // round-tripped back to the API on subsequent turns (DeepSeek V4).
        if !current_thinking.is_empty() {
            assistant_content.push(InputContentBlock::Thinking {
                thinking: std::mem::take(&mut current_thinking),
                signature: None,
            });
        }
        for text in &text_blocks {
            assistant_content.push(InputContentBlock::Text {
                text: text.clone(),
            });
        }
        for tool in &tool_calls {
            let input: serde_json::Value = serde_json::from_str(&tool.input_json)
                .unwrap_or(serde_json::Value::Null);
            assistant_content.push(InputContentBlock::ToolUse {
                id: tool.id.clone(),
                name: tool.name.clone(),
                input,
            });
        }
        if !assistant_content.is_empty() {
            history.push(InputMessage {
                role: "assistant".to_string(),
                content: assistant_content,
            });
        }

        // If no tool calls, this is the final text response
        if tool_calls.is_empty() {
            let full_text = text_blocks.join("");
            on_event(ChatEvent::Done(full_text));
            break;
        }

        // Execute tools and build result message
        let mut tool_results: Vec<InputContentBlock> = Vec::new();
        for tool in &tool_calls {
            let input: serde_json::Value =
                serde_json::from_str(&tool.input_json).unwrap_or(serde_json::Value::Null);
            let result = registry.execute(&tool.name, input, &ctx).await;
            let is_error = result.is_err();
            let text = result.unwrap_or_else(|e| e);

            tool_results.push(InputContentBlock::ToolResult {
                tool_use_id: tool.id.clone(),
                content: vec![ToolResultContentBlock::Text { text }],
                is_error,
            });
        }

        history.push(InputMessage {
            role: "user".to_string(),
            content: tool_results,
        });
    }

    Ok(())
}
