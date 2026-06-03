import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { FileEntry } from "../types";

interface Props {
  wsRoot: string | null;
  files: FileEntry[];
  content: { path: string; content: string } | null;
  onReadFile: (path: string) => void;
}

export function WorkspaceTree({ wsRoot, files, content, onReadFile }: Props) {
  const [localFiles, setLocalFiles] = useState<FileEntry[]>([]);
  const [currentDir, setCurrentDir] = useState<string>("");

  const displayFiles = currentDir === "" && files.length > 0 ? files : localFiles;

  useEffect(() => {
    if (wsRoot) {
      invoke<FileEntry[]>("list_workspace_files", { path: currentDir || null })
        .then(setLocalFiles)
        .catch(console.error);
    } else {
      setLocalFiles([]);
    }
  }, [wsRoot, currentDir]);

  const handleClick = (entry: FileEntry) => {
    if (entry.is_dir) {
      setCurrentDir((prev) => (prev ? `${prev}/${entry.name}` : entry.name));
    } else {
      const filePath = currentDir ? `${currentDir}/${entry.name}` : entry.path;
      onReadFile(filePath);
    }
  };

  const handleBack = () => {
    if (!currentDir) return;
    const parts = currentDir.split("/");
    parts.pop();
    setCurrentDir(parts.join("/"));
  };

  if (!wsRoot) {
    return (
      <div className="placeholder">
        <p>尚未选择工作区</p>
        <p>点击顶部 "📁 选择工作区" 开始</p>
      </div>
    );
  }

  return (
    <div className="workspace-tree">
      <div className="ws-root-label" title={wsRoot}>
        📂 {wsRoot}
        {currentDir && (
          <>
            <span className="ws-separator">/</span>
            {currentDir}
          </>
        )}
      </div>
      {currentDir !== "" && (
        <div className="file-entry dir-up" onClick={handleBack}>
          <span className="file-icon">📁</span>
          <span className="file-name">..</span>
        </div>
      )}
      <div className="file-list">
        {displayFiles.map((entry) => (
          <div key={entry.path} className="file-entry" onClick={() => handleClick(entry)}>
            <span className="file-icon">{entry.is_dir ? "📁" : "📄"}</span>
            <span className="file-name">{entry.name}</span>
            {!entry.is_dir && (
              <span className="file-size">{formatSize(entry.size)}</span>
            )}
          </div>
        ))}
        {displayFiles.length === 0 && (
          <div className="placeholder">(空目录)</div>
        )}
      </div>
      {content && (
        <div className="file-content-preview">
          <div className="preview-header">📋 {content.path}</div>
          <pre className="preview-body">{content.content.slice(0, 2000)}</pre>
        </div>
      )}
    </div>
  );
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
