import { useState, useCallback } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { ChatMessage } from "../types";

interface Props {
  messages: ChatMessage[];
  streaming: string;
  sending: boolean;
  error: string | null;
  input: string;
  onInputChange: (value: string) => void;
  onSend: () => void;
}

function formatTime(ts: number): string {
  const d = new Date(ts);
  const now = new Date();
  const isToday = d.toDateString() === now.toDateString();
  const time = d.toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" });
  return isToday ? time : `${d.toLocaleDateString("zh-CN")} ${time}`;
}

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);
  const handleCopy = useCallback(() => {
    navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  }, [text]);
  return (
    <button
      className={`msg-action-btn ${copied ? "copied" : ""}`}
      onClick={handleCopy}
      title="复制"
    >
      {copied ? "✓ 已复制" : "📋"}
    </button>
  );
}

export function ChatPanel({ messages, streaming, sending, error, input, onInputChange, onSend }: Props) {
  return (
    <div className="chat-panel-inner">
      <div className="chat-messages">
        {messages.map((msg, i) => (
          <div key={i} className={`chat-msg chat-${msg.role}`}>
            <div className="msg-card">
              <div className="msg-row">
                <div className="msg-avatar">
                  {msg.role === "user" ? "🧑" : "🦞"}
                </div>
                <span className="msg-role">
                  {msg.role === "user" ? "你" : "VIBE Paper"}
                </span>
                {msg.model && (
                  <span className="msg-model-badge">{msg.model}</span>
                )}
                <span className="msg-time">{formatTime(msg.timestamp)}</span>
                <div className="msg-actions">
                  <CopyButton text={msg.content} />
                </div>
              </div>
              <div className="msg-body">
                <ReactMarkdown remarkPlugins={[remarkGfm]}>
                  {msg.content}
                </ReactMarkdown>
              </div>
            </div>
          </div>
        ))}

        {streaming && (
          <div className="chat-msg chat-assistant">
            <div className="msg-card">
              <div className="msg-row">
                <div className="msg-avatar">🦞</div>
                <span className="msg-role">VIBE Paper</span>
                {sending && (
                  <span className="msg-model-badge">生成中</span>
                )}
              </div>
              <div className="msg-body">
                <ReactMarkdown remarkPlugins={[remarkGfm]}>
                  {streaming}
                </ReactMarkdown>
                <span className="streaming-cursor" />
              </div>
            </div>
          </div>
        )}

        {error && (
          <div className="chat-msg chat-error">
            <div className="msg-card">
              <div className="msg-row">
                <div className="msg-avatar">❌</div>
                <span className="msg-role">发生错误</span>
              </div>
              <div className="msg-body">{error}</div>
            </div>
          </div>
        )}
      </div>

      {/* Inline input area */}
      <div className="chat-input-area">
        <input
          type="text"
          className="chat-input-inline"
          placeholder="输入问题，Ctrl+Enter 发送..."
          value={input}
          onChange={(e) => onInputChange(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
              onSend();
            }
          }}
        />
        <button
          className="btn btn-primary btn-sm-send"
          disabled={sending || !input.trim()}
          onClick={onSend}
        >
          {sending ? "..." : "发送"}
        </button>
      </div>
    </div>
  );
}
