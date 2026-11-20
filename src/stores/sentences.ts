// 本次阅读拆过的长难句（最新的在前）；换文章就清空。结果本身由 Rust 按句缓存，这里只管显示。

import { create } from "zustand";
import { aiKeyStatus } from "../ipc/ai";
import { sentenceBreakdown, type BreakdownResult } from "../ipc/sentence";

export interface SentenceItem {
  id: number;
  sentence: string;
  before: string | null;
  after: string | null;
  status: "loading" | "done" | "error" | "nokey";
  result?: BreakdownResult;
  error?: string;
}

interface SentencesState {
  articleId: number | null;
  items: SentenceItem[];
  /** 展开着的那一条 */
  open: number | null;
  reset: (articleId: number | null) => void;
  run: (sentence: string, before: string | null, after: string | null, force?: boolean) => Promise<void>;
  setOpen: (id: number | null) => void;
}

let nextId = 1;

export const useSentences = create<SentencesState>((set, get) => ({
  articleId: null,
  items: [],
  open: null,
  reset: (articleId) => {
    if (get().articleId !== articleId) set({ articleId, items: [], open: null });
  },
  setOpen: (open) => set({ open }),
  run: async (sentence, before, after, force = false) => {
    const same = get().items.find((x) => x.sentence === sentence);
    if (same && !force && same.status !== "error" && same.status !== "nokey") {
      // 拆过的：挪到最前、展开
      set((s) => ({ items: [same, ...s.items.filter((x) => x !== same)], open: same.id }));
      return;
    }
    const id = same?.id ?? nextId++;
    const item: SentenceItem = { id, sentence, before, after, status: "loading" };
    set((s) => ({ items: [item, ...s.items.filter((x) => x.id !== id)], open: id }));
    const patch = (p: Partial<SentenceItem>) => set((s) => ({ items: s.items.map((x) => (x.id === id ? { ...x, ...p } : x)) }));
    try {
      const result = await sentenceBreakdown(sentence, before, after, force);
      patch({ status: "done", result, error: undefined });
    } catch (e) {
      // 没填 Key：给去设置的提示，而不是一串错误（缓存命中时用不到 Key，所以出错后再查）
      const hasKey = await aiKeyStatus().catch(() => true);
      patch(hasKey ? { status: "error", error: String(e) } : { status: "nokey" });
    }
  },
}));
