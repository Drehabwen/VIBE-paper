interface Props {
  onPromptClick: (prompt: string) => void;
}

const SUGGESTIONS = [
  {
    icon: "🔬",
    text: "帮我检索阿尔茨海默病的最新研究进展",
    tag: "文献检索",
  },
  {
    icon: "💊",
    text: "二甲双胍的作用机制和临床应用有哪些",
    tag: "药物查询",
  },
  {
    icon: "📋",
    text: "用 PRISMA 框架帮我设计一个系统综述的检索策略",
    tag: "研究设计",
  },
  {
    icon: "🧬",
    text: "解释一下 CRISPR-Cas9 基因编辑技术的原理",
    tag: "术语解释",
  },
  {
    icon: "📝",
    text: "帮我用 Vancouver 格式引用这篇关于 COVID-19 的论文",
    tag: "引用格式",
  },
  {
    icon: "📊",
    text: "帮我分析这篇临床试验的统计方法是否合理",
    tag: "文献分析",
  },
];

export function WelcomeScreen({ onPromptClick }: Props) {
  return (
    <div className="welcome-screen">
      <div className="welcome-hero">
        <div className="brand-icon-lg">
          <svg viewBox="0 0 48 48" fill="none">
            <rect width="48" height="48" rx="12" fill="#e94560" opacity="0.15" />
            <path
              d="M14 34V14h6l4 12 4-12h6v20h-5V21l-4 13h-2l-4-13v13h-5z"
              fill="#e94560"
            />
          </svg>
        </div>
        <h1>VIBE Paper</h1>
        <p>AI 驱动的医学科研助手 — 文献检索、论文分析、写作辅助</p>
      </div>

      <div className="welcome-suggestions">
        <h2>试试这些</h2>
        <div className="suggestion-grid">
          {SUGGESTIONS.map((s, i) => (
            <div
              key={i}
              className="suggestion-card"
              onClick={() => onPromptClick(s.text)}
            >
              <span className="suggestion-icon">{s.icon}</span>
              <span className="suggestion-text">{s.text}</span>
              <span className="suggestion-tag">{s.tag}</span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
