import { useEditor, EditorContent, BubbleMenu } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import Placeholder from "@tiptap/extension-placeholder";
import type { EditorStats } from "../types";

interface Props {
  content: string;
  onContentChange: (html: string) => void;
  onStatsChange: (stats: EditorStats) => void;
}

function computeStats(html: string, text: string): EditorStats {
  // CJK-aware word count
  const cjkChars = text.replace(/[^一-鿿㐀-䶿]/g, "");
  const latinWords = text
    .replace(/[一-鿿㐀-䶿]/g, " ")
    .split(/\s+/)
    .filter(Boolean);
  const wordCount = cjkChars.length + latinWords.length;

  // Citation count: look for PMID: patterns or [@...] references
  const citationCount =
    (html.match(/PMID:\s*\d+/g) || []).length +
    (html.match(/\[@[\w-]+\]/g) || []).length;

  return {
    wordCount,
    charCount: text.length,
    citationCount,
  };
}

const BUBBLE_BUTTONS = [
  { action: "toggleBold", label: "B", title: "粗体 (Ctrl+B)", check: "bold" },
  { action: "toggleItalic", label: "I", title: "斜体 (Ctrl+I)", check: "italic" },
  { action: "toggleStrike", label: "S̶", title: "删除线", check: "strike" },
  { action: "toggleCode", label: "<>", title: "行内代码", check: "code" },
  null, // separator
  { action: "toggleHeading", level: 1, label: "H1", title: "一级标题" },
  { action: "toggleHeading", level: 2, label: "H2", title: "二级标题" },
  { action: "toggleHeading", level: 3, label: "H3", title: "三级标题" },
  null,
  { action: "toggleBlockquote", label: "❝", title: "引用", check: "blockquote" },
  { action: "toggleBulletList", label: "•", title: "无序列表", check: "bulletList" },
  { action: "toggleOrderedList", label: "1.", title: "有序列表", check: "orderedList" },
];

export function EditorPanel({ content, onContentChange, onStatsChange }: Props) {
  const editor = useEditor({
    extensions: [
      StarterKit.configure({
        heading: { levels: [1, 2, 3] },
      }),
      Placeholder.configure({
        placeholder: "开始写作... 或点击右侧欢迎页提问",
        emptyEditorClass: "is-editor-empty",
      }),
    ],
    content,
    editorProps: {
      attributes: {
        class: "tiptap-editor",
      },
      // CJK IME: Tiptap v2 handles composition natively via ProseMirror.
      // If issues surface (Sogou IME, etc.), add compositionstart/compositionend
      // guards by checking editor.view.composing flag.
    },
    onUpdate: ({ editor }) => {
      const html = editor.getHTML();
      const text = editor.getText();
      const stats = computeStats(html, text);
      onContentChange(html);
      onStatsChange(stats);
    },
  });

  // Sync external content changes (e.g., when switching documents)
  // Only update if editor content differs from external content (avoid loops)
  // This is intentionally simple — full two-way binding later

  if (!editor) {
    return (
      <div className="editor-panel">
        <div className="tiptap-editor placeholder">
          <p>编辑器加载中...</p>
        </div>
      </div>
    );
  }

  return (
    <div className="editor-panel">
      {editor && (
        <BubbleMenu
          editor={editor}
          tippyOptions={{ placement: "top", duration: 150, maxWidth: 500 }}
        >
          <div className="bubble-menu">
            {BUBBLE_BUTTONS.map((btn, i) => {
              if (btn === null) {
                return (
                  <span key={i} className="bubble-sep">
                    |
                  </span>
                );
              }
              const isActive =
                "level" in btn
                  ? editor.isActive("heading", { level: btn.level })
                  : editor.isActive(btn.check!);

              return (
                <button
                  key={i}
                  className={isActive ? "active" : ""}
                  title={btn.title}
                  onClick={() => {
                    if ("level" in btn) {
                      editor
                        .chain()
                        .focus()
                        .toggleHeading({ level: btn.level as 1 | 2 | 3 })
                        .run();
                    } else {
                      (editor.chain().focus() as any)[btn.action]().run();
                    }
                  }}
                >
                  {btn.label}
                </button>
              );
            })}
          </div>
        </BubbleMenu>
      )}
      <EditorContent editor={editor} />
    </div>
  );
}
