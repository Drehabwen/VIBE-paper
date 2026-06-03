import type { LibraryTab } from "../types";

interface Props {
  active: LibraryTab;
  onChange: (tab: LibraryTab) => void;
}

const TABS: { id: LibraryTab; label: string; icon: string }[] = [
  { id: "papers", label: "文献", icon: "📄" },
  { id: "notes", label: "笔记", icon: "📝" },
  { id: "files", label: "文件", icon: "📁" },
];

export function TabBar({ active, onChange }: Props) {
  return (
    <div className="tab-bar">
      {TABS.map((t) => (
        <button
          key={t.id}
          className={`tab-btn ${active === t.id ? "tab-active" : ""}`}
          onClick={() => onChange(t.id)}
        >
          <span className="tab-icon">{t.icon}</span> {t.label}
        </button>
      ))}
    </div>
  );
}
