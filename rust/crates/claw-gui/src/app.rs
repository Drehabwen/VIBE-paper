use std::path::PathBuf;
use std::sync::Mutex;

use crate::backend::{ChatBackend, FileEntry, StreamEvent};
use crate::workspace::WorkspaceConfig;
use eframe::egui;
use medical_core::types::Paper;
use tokio::sync::mpsc;

#[derive(PartialEq)]
enum AppMode {
    Placeholder,
    Chatting,
}

#[derive(Clone, Copy, PartialEq)]
enum LeftPanelTab {
    Workspace,
    Papers,
    Notes,
}

pub struct ClawMdApp {
    backend: ChatBackend,
    messages: Vec<ChatMessage>,
    input: String,
    mode: AppMode,
    stream_rx: Option<mpsc::UnboundedReceiver<StreamEvent>>,
    streaming_text: String,
    current_model: String,
    available_models: Vec<String>,
    error_text: Option<String>,
    search_results: Vec<Paper>,
    selected_paper: Option<Paper>,
    workspace_root: Option<PathBuf>,
    workspace_files: Vec<FileEntry>,
    workspace_selected_file: Option<String>,
    workspace_file_content: Option<String>,
    show_workspace_picker: bool,
    left_panel_tab: LeftPanelTab,
    workspace_input: String,
}

#[derive(Clone)]
struct ChatMessage {
    role: String,
    content: String,
}

fn load_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    let zh_font_paths = [
        "C:\\Windows\\Fonts\\msyh.ttc",
        "C:\\Windows\\Fonts\\simhei.ttf",
    ];

    let mut loaded = false;
    for path in &zh_font_paths {
        if let Ok(bytes) = std::fs::read(path) {
            let font_data =
                std::sync::Arc::new(egui::FontData::from_owned(bytes).tweak(
                    egui::FontTweak {
                        scale: 1.0,
                        ..Default::default()
                    },
                ));
            fonts.font_data.insert("zh_font".to_owned(), font_data);
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "zh_font".to_owned());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .insert(0, "zh_font".to_owned());
            loaded = true;
            break;
        }
    }

    if !loaded {
        eprintln!("Warning: No Chinese font found. CJK characters may not render correctly.");
    }

    ctx.set_fonts(fonts);
}

impl ClawMdApp {
    pub fn new(_cc: &eframe::CreationContext<'_>, workspace_root: Option<PathBuf>) -> Self {
        load_fonts(&_cc.egui_ctx);

        let backend = ChatBackend::new();
        let default_model = backend
            .default_model()
            .map(|m| m.name)
            .unwrap_or_else(|| "claude-sonnet-4-6".to_string());

        let available_models = backend
            .all_models()
            .iter()
            .map(|m| m.name.clone())
            .collect();

        _cc.egui_ctx.set_visuals(egui::Visuals::dark());

        // If we have a saved workspace, set it in backend
        if let Some(ref root) = workspace_root {
            backend.set_workspace_root(Some(root.clone()));
        }

        Self {
            backend,
            messages: vec![ChatMessage {
                role: "assistant".to_string(),
                content: "欢迎使用 Galen！我是你的科研助手。\n\n你可以直接问我问题，我会帮你检索文献、解释术语、格式化引用。\n\n试试问我：\n• 帮我查一下阿尔茨海默病的最新研究\n• 解释一下什么是单核苷酸多态性\n• 用 Vancouver 格式引用这篇 PMID: 12345678".to_string(),
            }],
            input: String::new(),
            mode: AppMode::Placeholder,
            stream_rx: None,
            streaming_text: String::new(),
            current_model: default_model,
            available_models,
            error_text: None,
            search_results: Vec::new(),
            selected_paper: None,
            workspace_root,
            workspace_files: Vec::new(),
            workspace_selected_file: None,
            workspace_file_content: None,
            show_workspace_picker: false,
            left_panel_tab: LeftPanelTab::Papers,
            workspace_input: String::new(),
        }
    }

    fn send_message(&mut self) {
        let text = self.input.trim().to_string();
        if text.is_empty() {
            return;
        }

        self.messages.push(ChatMessage {
            role: "user".to_string(),
            content: text.clone(),
        });

        self.input.clear();
        self.mode = AppMode::Chatting;
        self.streaming_text.clear();
        self.error_text = None;

        let (tx, rx) = mpsc::unbounded_channel();
        self.stream_rx = Some(rx);

        let model_alias = self.current_model.clone();
        let model_id = self.backend.resolve_model(&model_alias);
        let history: Vec<_> = self
            .messages
            .iter()
            .filter(|m| m.role == "user" || m.role == "assistant")
            .map(|m| api::InputMessage {
                role: m.role.clone(),
                content: vec![api::InputContentBlock::Text {
                    text: m.content.clone(),
                }],
            })
            .collect();

        // Update workspace root in backend before sending
        self.backend.set_workspace_root(self.workspace_root.clone());

        let medical = self.backend.medical.clone();
        let router = self.backend.router.clone();
        ChatBackend::spawn_chat(model_alias, model_id, text, history, tx, medical, router, Mutex::new(self.workspace_root.clone()));
    }

    fn poll_stream(&mut self) {
        if let Some(ref mut rx) = self.stream_rx {
            loop {
                match rx.try_recv() {
                    Ok(StreamEvent::Delta(text)) => {
                        self.streaming_text.push_str(&text);
                    }
                    Ok(StreamEvent::Done(text)) => {
                        self.messages.push(ChatMessage {
                            role: "assistant".to_string(),
                            content: text,
                        });
                        self.streaming_text.clear();
                        self.stream_rx = None;
                        self.mode = AppMode::Placeholder;
                        break;
                    }
                    Ok(StreamEvent::SearchResults(papers)) => {
                        self.search_results = papers;
                    }
                    Ok(StreamEvent::WorkspaceRoot(path)) => {
                        self.workspace_root = Some(PathBuf::from(path));
                    }
                    Ok(StreamEvent::WorkspaceFileList(files)) => {
                        self.workspace_files = files;
                    }
                    Ok(StreamEvent::WorkspaceFileContent { path, content }) => {
                        self.workspace_selected_file = Some(path);
                        self.workspace_file_content = Some(content);
                    }
                    Ok(StreamEvent::Error(e)) => {
                        self.error_text = Some(e);
                        self.stream_rx = None;
                        self.mode = AppMode::Placeholder;
                        break;
                    }
                    Err(mpsc::error::TryRecvError::Empty) => break,
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        if !self.streaming_text.is_empty() {
                            self.messages.push(ChatMessage {
                                role: "assistant".to_string(),
                                content: std::mem::take(&mut self.streaming_text),
                            });
                        }
                        self.stream_rx = None;
                        self.mode = AppMode::Placeholder;
                        break;
                    }
                }
            }
        }
    }
}

impl eframe::App for ClawMdApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_stream();

        // Ctrl+Enter to send
        if ctx.input(|i| i.key_pressed(egui::Key::Enter)) && ctx.input(|i| i.modifiers.ctrl) {
            self.send_message();
        }

        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("🦞 Galen");
                ui.separator();
                ui.label("模型:");
                egui::ComboBox::from_id_salt("model_select")
                    .selected_text(&self.current_model)
                    .show_ui(ui, |ui| {
                        for model in &self.available_models.clone() {
                            ui.selectable_value(
                                &mut self.current_model,
                                model.clone(),
                                model,
                            );
                        }
                    });
                ui.separator();

                // Workspace section
                if ui.button("📁 选择工作区").clicked() {
                    self.show_workspace_picker = true;
                }
                if let Some(ref root) = self.workspace_root {
                    let dir_name = root
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "?".to_string());
                    ui.label(format!("📂 {}", dir_name));
                }

                ui.separator();
                if self.mode == AppMode::Chatting {
                    ui.spinner();
                    ui.label("思考中...");
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("新对话").clicked() {
                        self.messages.clear();
                        self.messages.push(ChatMessage {
                            role: "assistant".to_string(),
                            content: "新对话已开始。有什么可以帮你的？".to_string(),
                        });
                    }
                });
            });

            // Workspace picker inline
            if self.show_workspace_picker {
                ui.horizontal(|ui| {
                    ui.label("工作区路径:");
                    let response = ui.text_edit_singleline(&mut self.workspace_input);
                    if response.lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    {
                        let path = PathBuf::from(self.workspace_input.trim());
                        if path.exists() && path.is_dir() {
                            self.workspace_root = Some(path.clone());
                            self.backend.set_workspace_root(Some(path.clone()));
                            let path_display = format!("{}", path.display());
                            let _ = self.backend
                                .send_workspace_event(path_display.clone());
                            // Persist workspace choice
                            let mut config = WorkspaceConfig::load();
                            config.set_workspace(&path_display);
                        }
                        self.show_workspace_picker = false;
                        self.workspace_input.clear();
                    }
                    if ui.button("取消").clicked() {
                        self.show_workspace_picker = false;
                    }
                });
            }
        });

        egui::TopBottomPanel::bottom("bottom_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let response = ui.add_sized(
                    [ui.available_width() - 80.0, 32.0],
                    egui::TextEdit::singleline(&mut self.input)
                        .hint_text("输入消息，Ctrl+Enter 发送...")
                        .desired_width(f32::INFINITY),
                );
                if response.lost_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                    self.send_message();
                }
                if ui.button("发送").clicked() || {
                    let enter_pressed = ctx.input(|i| i.key_pressed(egui::Key::Enter));
                    let has_focus = response.has_focus();
                    enter_pressed && has_focus && !ctx.input(|i| i.modifiers.ctrl)
                } {
                    self.send_message();
                    response.request_focus();
                }
                ui.add_space(8.0);
                let sending = self.mode == AppMode::Chatting;
                if ui.add_enabled(!sending && !self.input.trim().is_empty(), egui::Button::new("发送")).clicked() {
                    self.send_message();
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.columns(2, |columns| {
                // LEFT: Tabbed panel (Workspace / Papers / Notes)
                columns[0].vertical(|ui| {
                    ui.add_space(4.0);

                    // Tab bar
                    ui.horizontal(|ui| {
                        ui.selectable_value(
                            &mut self.left_panel_tab,
                            LeftPanelTab::Workspace,
                            "📁 工作区",
                        );
                        ui.selectable_value(
                            &mut self.left_panel_tab,
                            LeftPanelTab::Papers,
                            "📄 文献",
                        );
                        ui.selectable_value(
                            &mut self.left_panel_tab,
                            LeftPanelTab::Notes,
                            "📝 笔记",
                        );
                    });
                    ui.separator();

                    match self.left_panel_tab {
                        LeftPanelTab::Workspace => {
                            if self.workspace_root.is_none() {
                                ui.add_space(20.0);
                                ui.label("尚未选择工作区");
                                ui.label("点击顶部 \"📁 选择工作区\" 开始");
                            } else {
                                let root_label = self
                                    .workspace_root
                                    .as_ref()
                                    .map(|p| p.display().to_string())
                                    .unwrap_or_default();
                                ui.label(format!("📂 {}", root_label));
                                ui.separator();
                                egui::ScrollArea::vertical()
                                    .auto_shrink([false, false])
                                    .show(ui, |ui| {
                                        // Clone to avoid borrow conflict when calling send_message
                                        let workspace_entries = self.workspace_files.clone();
                                        let mut pending_send = false;
                                        for entry in &workspace_entries {
                                            let icon = if entry.is_dir { "📁" } else { "📄" };
                                            if ui
                                                .selectable_label(
                                                    self.workspace_selected_file.as_deref()
                                                        == Some(&entry.path),
                                                    format!("{} {}", icon, entry.name),
                                                )
                                                .clicked()
                                            {
                                                self.workspace_selected_file =
                                                    Some(entry.path.clone());
                                                self.input = format!(
                                                    "请读取工作区文件: {}",
                                                    entry.path
                                                );
                                                pending_send = true;
                                            }
                                        }
                                        if pending_send {
                                            self.send_message();
                                        }
                                    });
                            }
                        }
                        LeftPanelTab::Papers => {
                            ui.add_space(4.0);

                            if self.search_results.is_empty() {
                                ui.add_space(20.0);
                                ui.label("在聊天中提出医学问题");
                                ui.label("AI 会自动检索 PubMed");
                                ui.add_space(6.0);
                                ui.separator();
                                ui.add_space(6.0);
                                ui.label("💡 试试问:");
                                ui.label("\"帮我查阿尔茨海默病的最新综述\"");
                                ui.label("\"二甲双胍的作用机制是什么\"");
                            } else {
                                ui.label(format!(
                                    "找到 {} 篇文献",
                                    self.search_results.len()
                                ));
                                ui.add_space(4.0);
                                ui.separator();

                                egui::ScrollArea::vertical()
                                    .auto_shrink([false, false])
                                    .show(ui, |ui| {
                                        for paper in &self.search_results {
                                            let selected = self
                                                .selected_paper
                                                .as_ref()
                                                .is_some_and(|s| s.pmid == paper.pmid);

                                            let text = format!("{}", paper.title);
                                            let rich = if selected {
                                                egui::RichText::new(text)
                                                    .color(egui::Color32::from_rgb(
                                                        144, 238, 144,
                                                    ))
                                            } else {
                                                egui::RichText::new(text)
                                            };

                                            if ui.selectable_label(selected, rich).clicked() {
                                                self.selected_paper = Some(paper.clone());
                                            }

                                            // Show metadata below title
                                            let meta = format!(
                                                "{}  |  {} ({})",
                                                paper
                                                    .authors
                                                    .first()
                                                    .map(|a| a.to_string())
                                                    .unwrap_or_else(|| "?".into()),
                                                paper.journal.as_deref().unwrap_or("?"),
                                                paper.year.as_deref().unwrap_or("?")
                                            );
                                            ui.label(
                                                egui::RichText::new(meta)
                                                    .size(11.0)
                                                    .color(egui::Color32::GRAY),
                                            );
                                            ui.label(format!("PMID: {}", paper.pmid));
                                            ui.add_space(4.0);
                                        }
                                    });

                                // Show selected paper abstract
                                if let Some(ref paper) = self.selected_paper {
                                    ui.add_space(8.0);
                                    ui.separator();
                                    ui.add_space(4.0);
                                    ui.colored_label(
                                        egui::Color32::from_rgb(144, 238, 144),
                                        "📋 摘要",
                                    );
                                    ui.add_space(4.0);
                                    if let Some(ref abs) = paper.abstract_text {
                                        ui.label(abs);
                                    } else {
                                        ui.label("(无摘要)");
                                    }
                                }
                            }
                        }
                        LeftPanelTab::Notes => {
                            ui.add_space(20.0);
                            ui.label("笔记功能即将推出");
                            ui.label("AI 可以将研究笔记保存到工作区");
                        }
                    }
                });

                // RIGHT: Chat
                columns[1].vertical(|ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            for msg in &self.messages {
                                let (color, label) = match msg.role.as_str() {
                                    "user" => (
                                        egui::Color32::from_rgb(100, 149, 237),
                                        "🧑 你",
                                    ),
                                    "assistant" => (
                                        egui::Color32::from_rgb(144, 238, 144),
                                        "🤖 Galen",
                                    ),
                                    _ => (egui::Color32::GRAY, "❓"),
                                };

                                ui.horizontal(|ui| {
                                    ui.colored_label(color, label);
                                    ui.add_space(4.0);
                                });

                                let rich = egui::RichText::new(&msg.content).size(14.0);
                                ui.label(rich);
                                ui.add_space(8.0);
                                ui.separator();
                            }

                            if !self.streaming_text.is_empty() {
                                ui.horizontal(|ui| {
                                    ui.colored_label(
                                        egui::Color32::from_rgb(144, 238, 144),
                                        "🤖 Claw-MD",
                                    );
                                    ui.add_space(4.0);
                                    ui.spinner();
                                });
                                ui.label(&self.streaming_text);
                            }

                            if let Some(ref error) = self.error_text {
                                ui.colored_label(
                                    egui::Color32::RED,
                                    format!("❌ {error}"),
                                );
                            }
                        });
                });
            });
        });

        ctx.request_repaint();
    }
}
