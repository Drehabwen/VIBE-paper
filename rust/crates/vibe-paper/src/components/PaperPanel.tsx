import { useState, useCallback } from "react";
import type { Paper } from "../types";

interface Props {
  papers: Paper[];
}

function formatVancouver(paper: Paper): string {
  const authors = paper.authors.slice(0, 3).join(", ");
  const etAl = paper.authors.length > 3 ? " et al." : "";
  const year = paper.year ?? "?";
  const title = paper.title;
  const journal = paper.journal ?? "?";
  const pmid = paper.pmid;
  return `${authors}${etAl}. ${title}. ${journal}. ${year}. PMID: ${pmid}.`;
}

function CopyButton({ text, label }: { text: string; label?: string }) {
  const [copied, setCopied] = useState(false);
  const handleCopy = useCallback(() => {
    navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  }, [text]);
  return (
    <button
      className={`paper-detail-copy ${copied ? "copied" : ""}`}
      onClick={handleCopy}
    >
      {copied ? "✓ 已复制" : label ?? "📋 复制引用"}
    </button>
  );
}

export function PaperPanel({ papers }: Props) {
  const [selected, setSelected] = useState<Paper | null>(null);

  if (papers.length === 0) {
    return (
      <div className="placeholder">
        <p>在聊天中提出医学问题</p>
        <p>AI 会自动检索 PubMed</p>
        <hr />
        <p className="hint">💡 试试问:</p>
        <p className="hint">"帮我查阿尔茨海默病的最新综述"</p>
        <p className="hint">"二甲双胍的作用机制是什么"</p>
      </div>
    );
  }

  return (
    <div className="paper-panel">
      <div className="paper-count">找到 {papers.length} 篇文献</div>
      <div className="paper-list">
        {papers.map((paper) => (
          <div
            key={paper.pmid}
            className={`paper-item ${selected?.pmid === paper.pmid ? "paper-selected" : ""}`}
            onClick={() => setSelected(paper)}
          >
            <div className="paper-title">{paper.title}</div>
            <div className="paper-meta">
              {paper.journal && (
                <span className="paper-journal-badge">{paper.journal}</span>
              )}
              <span className="paper-year">{paper.year ?? "?"}</span>
              <span>· {paper.authors[0] ?? "?"}</span>
            </div>
            <div className="paper-pmid">PMID: {paper.pmid}</div>
          </div>
        ))}
      </div>

      {selected && (
        <div className="paper-detail">
          <div className="paper-detail-header">
            <div className="paper-detail-title">{selected.title}</div>
            <CopyButton text={selected.title} label="📋 标题" />
          </div>
          <div className="paper-authors">
            {selected.authors.join(", ")}
          </div>
          <div className="paper-citation-row">
            <span className="citation-tag">
              {selected.journal ?? "?"} · {selected.year ?? "?"}
            </span>
            <span className="citation-tag">PMID: {selected.pmid}</span>
            {selected.doi && (
              <span className="citation-tag">DOI: {selected.doi}</span>
            )}
          </div>
          <CopyButton text={formatVancouver(selected)} label="📋 Vancouver 引用" />
          <div style={{ marginTop: 14 }}>
            <div className="abstract-header">摘要</div>
            <div className="abstract-body">
              {selected.abstract_text ?? "(无摘要)"}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
