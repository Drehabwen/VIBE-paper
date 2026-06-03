import { TabBar } from "./TabBar";
import { PaperPanel } from "./PaperPanel";
import { WorkspaceTree } from "./WorkspaceTree";
import type { LibraryTab, FileEntry, Paper } from "../types";

interface Props {
  activeTab: LibraryTab;
  onTabChange: (tab: LibraryTab) => void;
  wsRoot: string | null;
  wsFileList: FileEntry[];
  wsFileContent: { path: string; content: string } | null;
  searchResults: Paper[];
  onReadFile: (path: string) => void;
}

export function LibraryPanel({
  activeTab,
  onTabChange,
  wsRoot,
  wsFileList,
  wsFileContent,
  searchResults,
  onReadFile,
}: Props) {
  return (
    <>
      <TabBar active={activeTab} onChange={onTabChange} />
      <div className="library-panel-content">
        {activeTab === "files" && (
          <WorkspaceTree
            wsRoot={wsRoot}
            files={wsFileList}
            content={wsFileContent}
            onReadFile={onReadFile}
          />
        )}
        {activeTab === "papers" && (
          <PaperPanel papers={searchResults} />
        )}
        {activeTab === "notes" && (
          <div className="placeholder">
            <p>笔记功能即将推出</p>
            <p>AI 可以将研究笔记保存到工作区</p>
          </div>
        )}
      </div>
    </>
  );
}
