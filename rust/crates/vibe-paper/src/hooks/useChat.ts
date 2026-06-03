import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ChatMessage, Paper, FileEntry } from "../types";

export function useChat() {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [streaming, setStreaming] = useState("");
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [searchResults, setSearchResults] = useState<Paper[]>([]);
  const [wsFileList, setWsFileList] = useState<FileEntry[]>([]);
  const [wsFileContent, setWsFileContent] = useState<{
    path: string;
    content: string;
  } | null>(null);
  const currentModel = useRef<string>("");
  const sendingRef = useRef(false);

  // Register event listeners once on mount, clean up on unmount
  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];

    const register = async () => {
      const ul1 = await listen<string>("chat-delta", (e) => {
        setStreaming((prev) => prev + e.payload);
      });
      const ul2 = await listen<string>("chat-done", (e) => {
        setMessages((prev) => [
          ...prev,
          {
            role: "assistant",
            content: e.payload,
            timestamp: Date.now(),
            model: currentModel.current,
          },
        ]);
        setStreaming("");
        setSending(false);
        sendingRef.current = false;
      });
      const ul3 = await listen<string>("chat-error", (e) => {
        setError(e.payload);
        setSending(false);
        sendingRef.current = false;
      });
      const ul4 = await listen<Paper[]>("search-results", (e) => {
        setSearchResults(e.payload);
      });
      const ul5 = await listen<FileEntry[]>("workspace-file-list", (e) => {
        setWsFileList(e.payload);
      });
      const ul6 = await listen<{ path: string; content: string }>(
        "workspace-file-content",
        (e) => {
          setWsFileContent(e.payload);
        }
      );
      unlisteners.push(ul1, ul2, ul3, ul4, ul5, ul6);
    };

    register();

    return () => {
      unlisteners.forEach((fn) => fn());
    };
  }, []);

  const send = useCallback(
    async (text: string, modelAlias: string) => {
      if (!text.trim() || sendingRef.current) return;

      currentModel.current = modelAlias;

      setMessages((prev) => [
        ...prev,
        { role: "user", content: text, timestamp: Date.now() },
      ]);
      setSending(true);
      sendingRef.current = true;
      setStreaming("");
      setError(null);

      try {
        await invoke("send_message", {
          message: text,
          modelAlias: modelAlias,
        });
      } catch (e) {
        setError(String(e));
        setSending(false);
        sendingRef.current = false;
      }
    },
    []
  );

  const clear = useCallback(() => {
    setMessages([]);
    setStreaming("");
    setError(null);
    setSearchResults([]);
  }, []);

  return {
    messages,
    streaming,
    sending,
    error,
    searchResults,
    wsFileList,
    wsFileContent,
    send,
    clear,
  };
}
