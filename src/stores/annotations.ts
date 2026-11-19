// 当前打开的文章 / 书的批注，以及撤销栈（只在这次阅读里有效，换文章就清空）。

import { create } from "zustand";
import { tr } from "../i18n";
import {
  addAnnotation,
  deleteAnnotation,
  listAnnotations,
  setAnnotationColor,
  setAnnotationNote,
  type Annotation,
  type AnnotationColor,
  type NewAnnotation,
} from "../ipc/annotations";

export interface AnnotationApi {
  list: (articleId: number) => Promise<Annotation[]>;
  add: (input: NewAnnotation) => Promise<Annotation>;
  setColor: (id: number, color: AnnotationColor) => Promise<Annotation>;
  setNote: (id: number, note: string | null) => Promise<Annotation>;
  remove: (id: number) => Promise<Annotation | null>;
}

let api: AnnotationApi = {
  list: listAnnotations,
  add: addAnnotation,
  setColor: setAnnotationColor,
  setNote: setAnnotationNote,
  remove: deleteAnnotation,
};

/** 测试时换成假的 */
export function setAnnotationApi(a: AnnotationApi) {
  api = a;
}

type Step = { kind: "add"; id: number } | { kind: "remove"; item: Annotation } | { kind: "color"; id: number; prev: AnnotationColor } | { kind: "note"; id: number; prev: string | null };

interface AnnotationsState {
  articleId: number | null;
  items: Annotation[];
  load: (articleId: number) => Promise<void>;
  add: (input: NewAnnotation) => Promise<Annotation>;
  recolor: (id: number, color: AnnotationColor) => Promise<void>;
  setNote: (id: number, note: string | null) => Promise<void>;
  remove: (id: number) => Promise<void>;
  /** 退回上一步，返回给用户看的提示；没有可撤销的返回 null */
  undo: () => Promise<string | null>;
}

let stack: Step[] = [];

const sorted = (items: Annotation[]) => [...items].sort((a, b) => a.chapter - b.chapter || a.createdAt - b.createdAt || a.id - b.id);

export const useAnnotations = create<AnnotationsState>((set, get) => {
  const replace = (row: Annotation) => set((s) => ({ items: s.items.map((x) => (x.id === row.id ? row : x)) }));

  return {
    articleId: null,
    items: [],
    load: async (articleId) => {
      stack = [];
      set({ articleId, items: [] });
      const items = await api.list(articleId);
      if (get().articleId === articleId) set({ items: sorted(items) });
    },
    add: async (input) => {
      const row = await api.add(input);
      set((s) => ({ items: sorted([...s.items, row]) }));
      stack.push({ kind: "add", id: row.id });
      return row;
    },
    recolor: async (id, color) => {
      const prev = get().items.find((x) => x.id === id)?.color;
      if (!prev || prev === color) return;
      replace(await api.setColor(id, color));
      stack.push({ kind: "color", id, prev });
    },
    setNote: async (id, note) => {
      const cur = get().items.find((x) => x.id === id);
      if (!cur) return;
      const clean = note?.trim() ? note : null;
      if ((cur.note ?? null) === clean) return;
      replace(await api.setNote(id, clean));
      stack.push({ kind: "note", id, prev: cur.note ?? null });
    },
    remove: async (id) => {
      const gone = await api.remove(id);
      if (!gone) return;
      set((s) => ({ items: s.items.filter((x) => x.id !== id) }));
      stack.push({ kind: "remove", item: gone });
    },
    undo: async () => {
      const step = stack.pop();
      if (!step) return null;
      switch (step.kind) {
        case "add":
          await api.remove(step.id);
          set((s) => ({ items: s.items.filter((x) => x.id !== step.id) }));
          return tr("已撤销标注");
        case "remove": {
          const { item } = step;
          const row = await api.add({ articleId: item.articleId, chapter: item.chapter, quote: item.quote, prefix: item.prefix, suffix: item.suffix, color: item.color, note: item.note, createdAt: item.createdAt });
          // 恢复后 id 变了：栈里还指着旧 id 的步骤跟过去
          for (const s of stack) if ("id" in s && s.id === item.id) s.id = row.id;
          set((s) => ({ items: sorted([...s.items, row]) }));
          return tr("已恢复标注");
        }
        case "color":
          replace(await api.setColor(step.id, step.prev));
          return tr("已撤销改色");
        case "note":
          replace(await api.setNote(step.id, step.prev));
          return tr("已撤销笔记修改");
      }
    },
  };
});
