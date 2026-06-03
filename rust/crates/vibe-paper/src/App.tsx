import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useChat } from "./hooks/useChat";
import { ChatPanel } from "./components/ChatPanel";
import { LibraryPanel } from "./components/LibraryPanel";
import { EditorPanel } from "./components/EditorPanel";
import { WelcomeScreen } from "./components/WelcomeScreen";
import { StatusBar } from "./components/StatusBar";
import type { ModelConfig, LibraryTab, DocumentState, EditorStats } from "./types";

const DEFAULT_LIBRARY_WIDTH = 260;
const MIN_LIBRARY_WIDTH = 180;
const MAX_LIBRARY_WIDTH = 400;
const DEFAULT_CHAT_WIDTH = 340;
const MIN_CHAT_WIDTH = 260;
const MAX_CHAT_WIDTH = 500;

export default function App() {
  const chat = useChat();
  const [input, setInput] = useState("");
  const [models, setModels] = useState<ModelConfig[]>([]);
  const [model, setModel] = useState("");
  const [libraryTab, setLibraryTab] = useState<LibraryTab>("papers");
  const [wsRoot, setWsRoot] = useState<string | null>(null);
  const [wsPickerOpen, setWsPickerOpen] = useState(false);
  const [wsInput, setWsInput] = useState("");

  // Panel sizing
  const [libraryWidth, setLibraryWidth] = useState(DEFAULT_LIBRARY_WIDTH);
  const [libraryCollapsed, setLibraryCollapsed] = useState(false);
  const [chatWidth, setChatWidth] = useState(DEFAULT_CHAT_WIDTH);
  const [chatCollapsed, setChatCollapsed] = useState(false);
  const resizeLibRef = useRef(false);
  const resizeChatRef = useRef(false);

  // Document state
  const [doc, setDoc] = useState<DocumentState>({
    content: "",
    title: "未命名文档",
    isDirty: false,
    filePath: null,
    wordCount: 0,
    citationCount: 0,
  });
  const [editorStats, setEditorStats] = useState<EditorStats>({
    wordCount: 0,
    charCount: 0,
    citationCount: 0,
  });

  // Load models and workspace on mount
  useEffect(() => {
    invoke<ModelConfig[]>("get_models")
      .then(setModels)
      .catch(console.error);
    invoke<string | null>("get_workspace_root")
      .then(setWsRoot)
      .catch(console.error);
  }, []);

  useEffect(() => {
    if (!model && models.length > 0) setModel(models[0].name);
  }, [models]);

  // ---- Resize: library (left handle) ----
  const onLibResizeDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    resizeLibRef.current = true;
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
    const onMove = (ev: MouseEvent) => {
      if (!resizeLibRef.current) return;
      const next = Math.min(MAX_LIBRARY_WIDTH, Math.max(MIN_LIBRARY_WIDTH, ev.clientX));
      setLibraryWidth(next);
    };
    const onUp = () => {
      resizeLibRef.current = false;
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
      document.removeEventListener("mousemove", onMove);
      document.removeEventListener("mouseup", onUp);
    };
    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", onUp);
  }, []);

  // ---- Resize: chat (right handle) ----
  const onChatResizeDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    resizeChatRef.current = true;
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
    const onMove = (ev: MouseEvent) => {
      if (!resizeChatRef.current) return;
      const next = Math.min(MAX_CHAT_WIDTH, Math.max(MIN_CHAT_WIDTH, window.innerWidth - ev.clientX));
      setChatWidth(next);
    };
    const onUp = () => {
      resizeChatRef.current = false;
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
      document.removeEventListener("mousemove", onMove);
      document.removeEventListener("mouseup", onUp);
    };
    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", onUp);
  }, []);

  // ---- Actions ----
  const handleSend = () => {
    if (!input.trim() || chat.sending) return;
    chat.send(input, model || "sonnet");
    setInput("");
  };

  const handlePromptClick = (prompt: string) => {
    chat.send(prompt, model || "sonnet");
  };

  const handleSetWorkspace = async () => {
    const path = wsInput.trim();
    if (!path) return;
    try {
      await invoke("set_workspace", { path });
      setWsRoot(path);
      setWsPickerOpen(false);
      setWsInput("");
    } catch (e) {
      alert(String(e));
    }
  };

  const handleContentChange = useCallback((html: string) => {
    setDoc((prev) => ({ ...prev, content: html, isDirty: true }));
  }, []);

  const handleStatsChange = useCallback((stats: EditorStats) => {
    setEditorStats(stats);
  }, []);

  const showWelcome = doc.content === "" && chat.messages.length === 0 && !chat.streaming;

  return (
    <div className="app-container">
      {/* Top Bar */}
      <div className="top-bar">
        <div className="app-brand">
          <span className="brand-icon">
            <svg viewBox="0 0 48 48" fill="none" width="22" height="22">
              <rect width="48" height="48" rx="12" fill="#e94560" opacity="0.15" />
              <path
                d="M14 34V14h6l4 12 4-12h6v20h-5V21l-4 13h-2l-4-13v13h-5z"
                fill="#e94560"
              />
            </svg>
          </span>
          VIBE Paper
        </div>

        <div className="top-bar-section">
          <span className="top-label">模型</span>
          <div className="model-chips">
            {models.map((m) => (
              <button
                key={m.name}
                className={`model-chip ${m.name === model ? "active" : ""}`}
                onClick={() => setModel(m.name)}
              >
                {m.name}
              </button>
            ))}
          </div>
        </div>

        <div className="top-bar-spacer" />

        {chat.sending && (
          <div className="thinking-badge">
            <span className="thinking-dot" />
            <span className="thinking-dot" />
            <span className="thinking-dot" />
            AI 思考中
          </div>
        )}

        {wsRoot && (
          <div className="ws-indicator" title={wsRoot} onClick={() => setWsPickerOpen(true)}>
            📂 {wsRoot.split(/[/\\]/).pop()}
          </div>
        )}

        <button className="btn btn-sm btn-ghost" onClick={() => setWsPickerOpen(true)}>
          {wsRoot ? "切换工作区" : "选择工作区"}
        </button>

        <button className="btn btn-sm btn-ghost" onClick={chat.clear}>新对话</button>

        <button className="btn btn-sm btn-ghost" onClick={() => setLibraryCollapsed((c) => !c)}>
          {libraryCollapsed ? "◀" : "◀"} 文献库
        </button>

        <button className="btn btn-sm btn-ghost" onClick={() => setChatCollapsed((c) => !c)}>
          {chatCollapsed ? "◀" : "▶"} 聊天
        </button>
      </div>

      {/* Workspace Picker Modal */}
      {wsPickerOpen && (
        <div
          className="ws-picker-overlay"
          onClick={(e) => { if (e.target === e.currentTarget) setWsPickerOpen(false); }}
        >
          <div className="ws-picker-card">
            <h3>选择工作区目录</h3>
            <div className="ws-picker-row">
              <input
                type="text"
                className="ws-input"
                placeholder="输入文件夹路径，如 D:\research\my-project"
                value={wsInput}
                onChange={(e) => setWsInput(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && handleSetWorkspace()}
                autoFocus
              />
            </div>
            <div style={{ display: "flex", gap: 8, justifyContent: "flex-end", marginTop: 16 }}>
              <button className="btn btn-ghost" onClick={() => setWsPickerOpen(false)}>取消</button>
              <button className="btn btn-primary" onClick={handleSetWorkspace}>确定</button>
            </div>
          </div>
        </div>
      )}

      {/* Main content: 3 panels */}
      <div className="main-content">
        {/* Left: Library */}
        {!libraryCollapsed && (
          <div className="library-panel" style={{ width: libraryWidth }}>
            <LibraryPanel
              activeTab={libraryTab}
              onTabChange={setLibraryTab}
              wsRoot={wsRoot}
              wsFileList={chat.wsFileList}
              wsFileContent={chat.wsFileContent}
              searchResults={chat.searchResults}
              onReadFile={(path) => {
                chat.send(`请读取工作区文件: ${path}`, model || "sonnet");
              }}
            />
          </div>
        )}

        {/* Resize handle 1: library | editor */}
        {!libraryCollapsed && (
          <div
            className={`resize-handle ${resizeLibRef.current ? "active" : ""}`}
            onMouseDown={onLibResizeDown}
          />
        )}

        {/* Center: Editor or Welcome */}
        <div className="editor-panel">
          {showWelcome ? (
            <WelcomeScreen onPromptClick={handlePromptClick} />
          ) : (
            <EditorPanel
              content={doc.content}
              onContentChange={handleContentChange}
              onStatsChange={handleStatsChange}
            />
          )}
        </div>

        {/* Resize handle 2: editor | chat */}
        {!chatCollapsed && (
          <div
            className={`resize-handle ${resizeChatRef.current ? "active" : ""}`}
            onMouseDown={onChatResizeDown}
          />
        )}

        {/* Right: Chat */}
        {!chatCollapsed && (
          <div className="chat-panel" style={{ width: chatWidth }}>
            <ChatPanel
              messages={chat.messages}
              streaming={chat.streaming}
              sending={chat.sending}
              error={chat.error}
              input={input}
              onInputChange={setInput}
              onSend={handleSend}
            />
          </div>
        )}
      </div>

      {/* Bottom Status Bar */}
      <StatusBar
        wordCount={editorStats.wordCount}
        citationCount={editorStats.citationCount}
        statusMessage={
          chat.sending
            ? "AI 思考中..."
            : doc.isDirty
            ? "未保存"
            : "已就绪"
        }
      />
    </div>
  );
}
