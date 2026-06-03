use std::sync::{Arc, OnceLock};

use api::{
    ContentBlockDelta, InputContentBlock, InputMessage, MessageRequest, OpenAiCompatClient,
    OpenAiCompatConfig, OutputContentBlock, ProviderClient, StreamEvent as ApiStreamEvent,
    ToolChoice, ToolDefinition, ToolResultContentBlock,
};
use medical_core::MedicalCore;
use medical_core::types::Paper;
use model_router::ModelRouter;
use serde_json::json;
use tokio::sync::mpsc;

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
4. 当用户要求格式化引用时，调用 format_citation。\n\
\n\
## 回答风格\n\
- 检索结果要按相关性整理，标注 PMID、作者、期刊、年份。\
- 解释术语时用医学生能理解的语言，给出临床相关性。\
- 引用格式化后给出可以直接复制使用的文本。";

#[derive(Debug, Clone)]
pub struct ModelConfig {
    pub name: String,
    pub model_id: String,
}

pub struct ChatBackend {
    pub router: ModelRouter,
    pub medical: Arc<MedicalCore>,
}

impl ChatBackend {
    pub fn new() -> Self {
        let router = ModelRouter::load().unwrap_or_else(|e| {
            eprintln!("Failed to load models.toml: {e}, using defaults");
            ModelRouter::default()
        });
        let medical = Arc::new(MedicalCore::new(None));
        Self { router, medical }
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
    ) {
        Self::runtime().spawn(async move {
            let result = Self::run_chat(
                &model_alias, &model_id, &user_message, conversation_history, &tx, &medical,
                &router,
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
                let result = Self::execute_tool(medical, &tool.name, input).await;
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
    ) -> Result<String, String> {
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
            _ => Err(format!("Unknown tool: {name}")),
        }
    }
}

#[derive(Debug, Clone)]
pub enum StreamEvent {
    Delta(String),
    Done(String),
    Error(String),
    SearchResults(Vec<Paper>),
}

struct PendingToolCall {
    id: String,
    name: String,
    input_json: String,
}
