export interface ModelConfig {
  name: string;
  model_id: string;
}

export interface FileEntry {
  name: string;
  path: string;
  is_dir: boolean;
  size: number;
}

export interface Paper {
  pmid: string;
  title: string;
  authors: string[];
  journal: string | null;
  year: string | null;
  doi: string | null;
  abstract_text: string | null;
}

export interface ChatMessage {
  role: "user" | "assistant";
  content: string;
  timestamp: number;
  model?: string;
}

export type LibraryTab = "papers" | "notes" | "files";

export interface DocumentState {
  /** HTML content from Tiptap editor */
  content: string;
  title: string;
  isDirty: boolean;
  filePath: string | null;
  wordCount: number;
  citationCount: number;
}

export interface EditorStats {
  wordCount: number;
  charCount: number;
  citationCount: number;
}
