use std::sync::{Arc, Mutex, OnceLock};

use api::{
    ContentBlockDelta, InputContentBlock, InputMessage, MessageRequest, OpenAiCompatClient,
    OpenAiCompatConfig, OutputContentBlock, ProviderClient, StreamEvent as ApiStreamEvent,
    ToolChoice, ToolDefinition, ToolResultContentBlock,
};
use medical_core::MedicalCore;
use medical_core::types::Paper;
use model_router::ModelRouter;
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use tokio::sync::mpsc;

const KNOWN_MODELS: &[(&str, &str)] = &[
    ("opus", "claude-opus-4-6"),
    ("sonnet", "claude-sonnet-4-6"),
    ("haiku", "claude-haiku-4-5-20251001"),
    ("gpt-4o", "gpt-4o"),
    ("gpt-4.1", "gpt-4.1"),
];

const MEDICAL_SYSTEM_PROMPT: &str = "\
你是 Galen，一个医学科研助手。你的任务是通过对话帮助医学研究人员完成文献检索、\
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

#[derive(Debug, Clone)]
pub struct ModelConfig {
    pub name: String,
    #[allow(dead_code)]
    pub model_id: String,
}

pub struct ChatBackend {
    pub router: ModelRouter,
    pub medical: Arc<MedicalCore>,
    pub workspace_root: Mutex<Option<PathBuf>>,
}

impl ChatBackend {
    pub fn new() -> Self {
        let router = ModelRouter::load().unwrap_or_else(|e| {
            eprintln!("Failed to load models.toml: {e}, using defaults");
            ModelRouter::default()
        });
        let medical = Arc::new(MedicalCore::new(None));
        Self { router, medical, workspace_root: Mutex::new(None) }
    }

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
    #[allow(dead_code)]
    pub fn get_workspace_root(&self) -> Option<PathBuf> {
        self.workspace_root.lock().ok()?.clone()
    }

    /// Send a workspace event from UI thread
    pub fn send_workspace_event(&self, _path: String) -> bool {
        // This needs to send a StreamEvent to the UI...
        // Since we're on the UI thread, we can't directly send.
        // Instead, this will be handled differently - the UI reads workspace_root
        // from the backend and sends the event itself.
        //
        // For now, just return true - the UI will handle sending events
        true
    }

    fn tool_definitions() -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "search_pubmed".into(),
                description: Some(
                    "Search PubMed for medical literature. Call this when the user asks about \
                     any medical topic, disease, drug, gene, or wants to find research papers. \
                     Returns a list of papers with PMID, title, authors, journal, and year."
                        .into(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "PubMed search query. Use MeSH terms and Boolean operators (AND, OR, NOT) for precision."
                        },
                        "max_results": {
                            "type": "integer",
                            "description": "Maximum results (1-20, default 10)"
                        }
                    },
                    "required": ["query"]
                }),
            },
            ToolDefinition {
                name: "fetch_article".into(),
                description: Some(
                    "Fetch detailed metadata for a specific PubMed article by PMID. \
                     Returns title, abstract, authors, journal, year, DOI, etc."
                        .into(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "pmid": {
                            "type": "string",
                            "description": "PubMed ID (PMID) of the article"
                        }
                    },
                    "required": ["pmid"]
                }),
            },
            ToolDefinition {
                name: "format_citation".into(),
                description: Some(
                    "Format one or more papers into a specific citation style. \
                     Use this when the user asks to format references."
                        .into(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "pmids": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "List of PubMed IDs to format"
                        },
                        "style": {
                            "type": "string",
                            "enum": ["apa", "vancouver", "bibtex", "ris", "mla"],
                            "description": "Citation style"
                        }
                    },
                    "required": ["pmids", "style"]
                }),
            },
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
            },
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
            },
            ToolDefinition {
                name: "read_file".into(),
                description: Some(
                    "Read the contents of a file from the workspace."
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
            },
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
            },
            ToolDefinition {
                name: "save_paper".into(),
                description: Some(
                    "Save paper metadata as a JSON file to the workspace papers/ directory."
                        .into(),
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
            },
            ToolDefinition {
                name: "delete_file".into(),
                description: Some("Delete a file from the workspace. Cannot delete directories (use delete_directory for that). Operation is irreversible.".into()),
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
            },
            ToolDefinition {
                name: "delete_directory".into(),
                description: Some("Recursively delete a directory and all its contents from the workspace. Operation is irreversible. Use with caution.".into()),
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
            },
            ToolDefinition {
                name: "move_file".into(),
                description: Some("Move or rename a file or directory within the workspace. Creates parent directories if needed.".into()),
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
            },
            ToolDefinition {
                name: "search_files".into(),
                description: Some("Search for files by glob pattern within the workspace (e.g. '**/*.json', '*.md'). Also supports grep-style content search with the 'grep' parameter.".into()),
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
            },
            ToolDefinition {
                name: "execute_command".into(),
                description: Some("Execute a shell command within the workspace directory. Use for running scripts (Python, R, etc.), data analysis, or file processing. Command runs in a shell environment with the workspace root as the working directory. Has a 30-second timeout.".into()),
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
            },
        ]
    }

    fn runtime() -> &'static tokio::runtime::Runtime {
        static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
        RT.get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("Failed to create tokio runtime")
        })
    }

    pub fn spawn_chat(
        model_alias: String,
        model_id: String,
        user_message: String,
        conversation_history: Vec<InputMessage>,
        tx: mpsc::UnboundedSender<StreamEvent>,
        medical: Arc<MedicalCore>,
        router: ModelRouter,
        workspace_root: std::sync::Mutex<Option<PathBuf>>,
    ) {
        Self::runtime().spawn(async move {
            let result = Self::run_chat(
                &model_alias, &model_id, &user_message, conversation_history, &tx, &medical,
                &router, &workspace_root,
            )
            .await;
            if let Err(e) = result {
                let _ = tx.send(StreamEvent::Error(format!("{e}")));
            }
        });
    }

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

    async fn run_chat(
        model_alias: &str,
        model_id: &str,
        user_message: &str,
        mut history: Vec<InputMessage>,
        tx: &mpsc::UnboundedSender<StreamEvent>,
        medical: &MedicalCore,
        router: &ModelRouter,
        workspace_root: &std::sync::Mutex<Option<PathBuf>>,
    ) -> Result<(), Box<dyn std::error::Error + Send>> {
        let client = Self::make_client(model_alias, model_id, router)?;

        history.push(InputMessage {
            role: "user".to_string(),
            content: vec![InputContentBlock::Text {
                text: user_message.to_string(),
            }],
        });

        let tools = Self::tool_definitions();

        // Multi-turn loop: keep going until model responds with text (no tool calls)
        let mut turn = 0;
        let max_turns = 5;
        loop {
            turn += 1;
            if turn > max_turns {
                let _ = tx.send(StreamEvent::Error("Reached max tool-call turns".into()));
                break;
            }

            let request = MessageRequest {
                model: model_id.to_string(),
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
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;

            // Collect content blocks for this response
            let mut text_blocks: Vec<String> = Vec::new();
            let mut tool_calls: Vec<PendingToolCall> = Vec::new();
            let mut current_tool: Option<PendingToolCall> = None;
            let mut current_text: String = String::new();

            loop {
                match stream.next_event().await {
                    Ok(Some(ApiStreamEvent::ContentBlockStart(event))) => {
                        match event.content_block {
                            OutputContentBlock::Text { text } => {
                                current_text = text;
                                if !current_text.is_empty() {
                                    let _ = tx.send(StreamEvent::Delta(current_text.clone()));
                                }
                            }
                            OutputContentBlock::ToolUse { id, name, .. } => {
                                // Flush any text before starting tool call
                                if !current_text.is_empty() {
                                    text_blocks.push(std::mem::take(&mut current_text));
                                }
                                current_tool = Some(PendingToolCall {
                                    id,
                                    name,
                                    input_json: String::new(),
                                });
                            }
                            OutputContentBlock::Thinking { .. } => {}
                            _ => {}
                        }
                    }
                    Ok(Some(ApiStreamEvent::ContentBlockDelta(event))) => {
                        match event.delta {
                            ContentBlockDelta::TextDelta { text } => {
                                current_text.push_str(&text);
                                let _ = tx.send(StreamEvent::Delta(text));
                            }
                            ContentBlockDelta::InputJsonDelta { partial_json } => {
                                if let Some(ref mut tool) = current_tool {
                                    tool.input_json.push_str(&partial_json);
                                }
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
                        let _ = tx.send(StreamEvent::Error(format!("stream error: {e}")));
                        return Ok(());
                    }
                    _ => {}
                }
            }

            // Build assistant message from collected blocks
            let mut assistant_content: Vec<InputContentBlock> = Vec::new();
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
                let _ = tx.send(StreamEvent::Done(full_text));
                break;
            }

            // Execute tools and build result message
            let mut tool_results: Vec<InputContentBlock> = Vec::new();
            for tool in &tool_calls {
                let input: serde_json::Value =
                    serde_json::from_str(&tool.input_json).unwrap_or(serde_json::Value::Null);
                let result = Self::execute_tool(medical, &tool.name, input, workspace_root, tx).await;
                let is_error = result.is_err();
                let text = result.unwrap_or_else(|e| e);

                // Send paper results to UI if this is a search
                if tool.name == "search_pubmed" {
                    if let Ok(input_val) = serde_json::from_str::<serde_json::Value>(&tool.input_json) {
                        let query = input_val["query"].as_str().unwrap_or("");
                        let limit = input_val["max_results"].as_u64().unwrap_or(10) as u32;
                        if let Ok(papers) = medical.search_pubmed(query, limit).await {
                            let _ = tx.send(StreamEvent::SearchResults(papers));
                        }
                    }
                }

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

    async fn execute_tool(
        medical: &MedicalCore,
        name: &str,
        input: serde_json::Value,
        workspace_root: &std::sync::Mutex<Option<PathBuf>>,
        tx: &mpsc::UnboundedSender<StreamEvent>,
    ) -> Result<String, String> {
        // Helper to resolve a relative path against the workspace root.
        fn resolve_workspace_path(
            workspace_root: &std::sync::Mutex<Option<PathBuf>>,
            rel: &str,
        ) -> Result<PathBuf, String> {
            let guard = workspace_root
                .lock()
                .map_err(|e| format!("Workspace lock error: {e}"))?;
            let root = guard
                .as_ref()
                .ok_or_else(|| "No workspace selected. Please select a workspace folder first.".to_string())?;
            let resolved = root.join(rel);
            // Ensure the resolved path is still within the workspace root
            let canonical = fs::canonicalize(&root).map_err(|e| format!("Cannot resolve workspace root: {e}"))?;
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

        match name {
            "search_pubmed" => {
                let query = input["query"]
                    .as_str()
                    .ok_or("Missing 'query' parameter")?;
                let limit = input["max_results"].as_u64().unwrap_or(10) as u32;
                let papers = medical
                    .search_pubmed(query, limit)
                    .await
                    .map_err(|e| format!("PubMed search error: {e}"))?;

                if papers.is_empty() {
                    Ok("No results found. Try broader search terms.".into())
                } else {
                    let summary: Vec<String> = papers
                        .iter()
                        .map(|p| {
                            let authors = if p.authors.is_empty() {
                                "Unknown".to_string()
                            } else if p.authors.len() == 1 {
                                p.authors[0].to_string()
                            } else {
                                format!("{} et al.", p.authors[0])
                            };
                            let journal = p.journal.as_deref().unwrap_or("Unknown Journal");
                            let year = p.year.as_deref().unwrap_or("n.d.");
                            let title = &p.title;
                            format!(
                                "PMID:{}\n  {}\n  {} — {} ({})\n",
                                p.pmid, title, authors, journal, year
                            )
                        })
                        .collect();
                    Ok(format!(
                        "Found {} results:\n\n{}",
                        papers.len(),
                        summary.join("\n")
                    ))
                }
            }
            "fetch_article" => {
                let pmid = input["pmid"]
                    .as_str()
                    .ok_or("Missing 'pmid' parameter")?;
                let paper = medical
                    .fetch_article(pmid)
                    .await
                    .map_err(|e| format!("PubMed fetch error: {e}"))?;

                match paper {
                    None => Ok(format!("No article found for PMID: {pmid}")),
                    Some(p) => {
                        let abstract_text = p
                            .abstract_text
                            .as_deref()
                            .unwrap_or("No abstract available");
                        Ok(format!(
                            "Title: {}\nAuthors: {}\nJournal: {}\nYear: {}\nDOI: {}\n\nAbstract:\n{}",
                            p.title,
                            p.authors.iter().map(|a| a.to_string()).collect::<Vec<_>>().join(", "),
                            p.journal.as_deref().unwrap_or("N/A"),
                            p.year.as_deref().unwrap_or("N/A"),
                            p.doi.as_deref().unwrap_or("N/A"),
                            abstract_text,
                        ))
                    }
                }
            }
            "format_citation" => {
                let pmids: Vec<String> = input["pmids"]
                    .as_array()
                    .ok_or("Missing 'pmids' parameter")?
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect();
                let style_str = input["style"]
                    .as_str()
                    .ok_or("Missing 'style' parameter")?;
                let style = medical_core::types::CitationStyle::from_str(style_str)
                    .ok_or(format!("Unknown citation style: {style_str}"))?;

                let papers = medical
                    .pubmed
                    .fetch_articles(&pmids)
                    .await
                    .map_err(|e| format!("PubMed fetch error: {e}"))?;

                let formatted = medical.format_citations(&papers, style);
                Ok(formatted)
            }
            "create_directory" => {
                let path = input["path"]
                    .as_str()
                    .ok_or("Missing 'path' parameter")?;
                let target = resolve_workspace_path(workspace_root, path)?;
                fs::create_dir_all(&target)
                    .map_err(|e| format!("Failed to create directory: {e}"))?;
                Ok(format!("Created: {}", target.display()))
            }
            "write_file" => {
                let path = input["path"]
                    .as_str()
                    .ok_or("Missing 'path' parameter")?;
                let content = input["content"]
                    .as_str()
                    .ok_or("Missing 'content' parameter")?;
                let target = resolve_workspace_path(workspace_root, path)?;
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|e| format!("Failed to create parent directories: {e}"))?;
                }
                let bytes = content.as_bytes();
                fs::write(&target, bytes)
                    .map_err(|e| format!("Failed to write file: {e}"))?;
                Ok(format!("Wrote {} bytes to {}", bytes.len(), target.display()))
            }
            "read_file" => {
                let path = input["path"]
                    .as_str()
                    .ok_or("Missing 'path' parameter")?;
                let target = resolve_workspace_path(workspace_root, path)?;
                let content = fs::read_to_string(&target)
                    .map_err(|e| format!("Failed to read file: {e}"))?;
                // Limit to 100KB
                let limited = if content.len() > 102_400 {
                    format!("{}...\n[File truncated at 100KB]", &content[..102_400])
                } else {
                    content.clone()
                };
                let _ = tx.send(StreamEvent::WorkspaceFileContent {
                    path: path.to_string(),
                    content: content.clone(),
                });
                Ok(limited)
            }
            "list_files" => {
                let path = input["path"]
                    .as_str()
                    .unwrap_or("");
                let target = resolve_workspace_path(workspace_root, path)?;
                let mut entries: Vec<FileEntry> = Vec::new();

                let dir_iter = fs::read_dir(&target)
                    .map_err(|e| format!("Failed to read directory: {e}"))?;
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
                        path: if path.is_empty() { rel } else { format!("{}/{}", path, rel) },
                        is_dir,
                        size,
                    });
                }

                // Sort: dirs first, then by name
                entries.sort_by(|a, b| {
                    b.is_dir.cmp(&a.is_dir).then_with(|| a.name.cmp(&b.name))
                });

                let _ = tx.send(StreamEvent::WorkspaceFileList(entries.clone()));

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
                        if path.is_empty() { "workspace root" } else { path },
                        listing.join("\n")
                    ))
                }
            }
            "save_paper" => {
                let pmid = input["pmid"]
                    .as_str()
                    .ok_or("Missing 'pmid' parameter")?;
                let format = input["format"]
                    .as_str()
                    .unwrap_or("json");

                // Fetch the paper metadata
                let paper = medical
                    .fetch_article(pmid)
                    .await
                    .map_err(|e| format!("PubMed fetch error: {e}"))?
                    .ok_or_else(|| format!("No article found for PMID: {pmid}"))?;

                // Ensure papers/ directory exists
                let papers_dir = resolve_workspace_path(workspace_root, "papers")?;
                fs::create_dir_all(&papers_dir)
                    .map_err(|e| format!("Failed to create papers directory: {e}"))?;

                let filename = format!("{}.{}", pmid, format);
                let target = papers_dir.join(&filename);

                let content = match format {
                    "json" => serde_json::to_string_pretty(&paper)
                        .map_err(|e| format!("Failed to serialize paper: {e}"))?,
                    _ => return Err(format!("Unsupported format: {format}. Use 'json'.")),
                };

                fs::write(&target, content)
                    .map_err(|e| format!("Failed to write paper file: {e}"))?;

                Ok(format!("Saved paper {} to papers/{}", pmid, filename))
            }
            "delete_file" => {
                let path = input["path"].as_str().ok_or("Missing 'path' parameter")?;
                let target = resolve_workspace_path(workspace_root, path)?;
                let meta = fs::metadata(&target).map_err(|e| format!("Cannot access file: {e}"))?;
                if meta.is_dir() {
                    return Err("Use delete_directory for directories, not delete_file.".into());
                }
                fs::remove_file(&target)
                    .map_err(|e| format!("Failed to delete file: {e}"))?;
                Ok(format!("Deleted file: {}", target.display()))
            }
            "delete_directory" => {
                let path = input["path"].as_str().ok_or("Missing 'path' parameter")?;
                let target = resolve_workspace_path(workspace_root, path)?;
                let meta =
                    fs::metadata(&target).map_err(|e| format!("Cannot access directory: {e}"))?;
                if !meta.is_dir() {
                    return Err("Path is not a directory. Use delete_file for files.".into());
                }
                fs::remove_dir_all(&target)
                    .map_err(|e| format!("Failed to delete directory: {e}"))?;
                Ok(format!("Deleted directory: {}", target.display()))
            }
            "move_file" => {
                let from = input["from"].as_str().ok_or("Missing 'from' parameter")?;
                let to = input["to"].as_str().ok_or("Missing 'to' parameter")?;
                let source = resolve_workspace_path(workspace_root, from)?;
                let dest = resolve_workspace_path(workspace_root, to)?;
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|e| format!("Failed to create parent directories: {e}"))?;
                }
                fs::rename(&source, &dest)
                    .map_err(|e| format!("Failed to move/rename: {e}"))?;
                Ok(format!(
                    "Moved {} -> {}",
                    source.display(),
                    dest.display()
                ))
            }
            "search_files" => {
                let pattern = input["pattern"]
                    .as_str()
                    .ok_or("Missing 'pattern' parameter")?;
                let grep = input["grep"].as_str().filter(|s| !s.is_empty());
                let search_path = input["path"].as_str().unwrap_or("");
                let target = resolve_workspace_path(workspace_root, search_path)?;

                let mut results: Vec<String> = Vec::new();
                // Use glob to find matching files
                let glob_pattern = target.join(pattern);
                let glob_str = glob_pattern.to_string_lossy().to_string();
                let paths = glob::glob(&glob_str)
                    .map_err(|e| format!("Invalid glob pattern: {e}"))?;
                for entry in paths {
                    match entry {
                        Ok(p) => {
                            let rel = p.strip_prefix(&target).unwrap_or(&p).to_string_lossy().to_string();
                            let meta = fs::metadata(&p).ok();
                            let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
                            let prefix = if is_dir { "[DIR] " } else { "[FILE]" };
                            if let Some(grep_text) = grep {
                                if !is_dir {
                                    if let Ok(content) = fs::read_to_string(&p) {
                                        if content.contains(grep_text) {
                                            let preview = content.lines()
                                                .filter(|l| l.contains(grep_text))
                                                .take(5)
                                                .collect::<Vec<_>>()
                                                .join("\n  ");
                                            results.push(format!("{} {} (matches)\n  {}", prefix, rel, preview));
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
                    Ok(format!("No files found matching '{}' in {}", pattern, if search_path.is_empty() { "workspace root" } else { search_path }))
                } else {
                    Ok(format!("Search results for '{}':\n{}", pattern, results.join("\n")))
                }
            }
            "execute_command" => {
                let command = input["command"]
                    .as_str()
                    .ok_or("Missing 'command' parameter")?;
                let ws_root = resolve_workspace_path(workspace_root, "")?;

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
                    result.push_str(&format!("\nstderr:\n{}", String::from_utf8_lossy(&output.stderr)));
                }
                if result.is_empty() {
                    result.push_str(&format!("Command completed with exit code: {}", output.status.code().unwrap_or(-1)));
                } else {
                    result.push_str(&format!("\nExit code: {}", output.status.code().unwrap_or(-1)));
                }
                Ok(result)
            }
            _ => Err(format!("Unknown tool: {name}")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    pub path: String, // relative to workspace root
    pub is_dir: bool,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub enum StreamEvent {
    Delta(String),
    Done(String),
    Error(String),
    SearchResults(Vec<Paper>),
    #[allow(dead_code)]
    WorkspaceRoot(String),                                  // workspace folder path changed
    WorkspaceFileList(Vec<FileEntry>),                      // file/directory listing
    WorkspaceFileContent { path: String, content: String }, // file read result
}

struct PendingToolCall {
    id: String,
    name: String,
    input_json: String,
}
