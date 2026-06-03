interface Props {
  wordCount: number;
  citationCount: number;
  statusMessage: string;
}

export function StatusBar({ wordCount, citationCount, statusMessage }: Props) {
  return (
    <div className="bottom-bar status-bar-compact">
      <span className="status-stat">📝 {wordCount} 字</span>
      <span className="status-separator">|</span>
      <span className="status-stat">📄 {citationCount} 引用</span>
      <span className="status-spacer" />
      <span className="status-message">{statusMessage}</span>
    </div>
  );
}
