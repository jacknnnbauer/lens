// 查词状态：当前查询、各词典卡片、未命中时的候选。
// 结果事件可能早于 lookup() 的返回到达，所以先按 requestId 缓冲，拿到句柄后再合并。

import { create } from "zustand";
import { prepareEntry, type PreparedEntry } from "../dict-render/prepare";
import { cancelLookup, lookup as ipcLookup, onLookupDone, onLookupResult } from "../ipc/lookup";
import type { LookupDoneEvent, LookupHandle, LookupResultEvent, LookupStatus } from "../ipc/types";

/** 默认展开前几部命中的词典（settings.layout.cardsExpanded = "first2"） */
export const DEFAULT_EXPANDED = 2;
const BUFFER_IDS = 4;

export interface CardEntry extends PreparedEntry {
  headword: string;
}

export interface Card {
  id: string;
  name: string;
  status: "pending" | LookupStatus;
  entries: CardEntry[];
  error: string | null;
  ms: number | null;
}

interface LookupState {
  handle: LookupHandle | null;
  cards: Card[];
  done: boolean;
  suggestions: string[];
  /** 用户手动展开/折叠过的卡片 */
  expanded: Record<string, boolean>;
  error: string | null;
  search: (query: string) => Promise<void>;
  clear: () => void;
  setExpanded: (id: string, open: boolean) => void;
}

type Pending = { results: LookupResultEvent[]; done: LookupDoneEvent | null };
const buffer = new Map<string, Pending>();
/** 只认最后一次 search 的返回，防止较早的查询晚到后覆盖新结果 */
let searchSeq = 0;

function buffered(id: string): Pending {
  let p = buffer.get(id);
  if (!p) {
    p = { results: [], done: null };
    buffer.set(id, p);
    while (buffer.size > BUFFER_IDS) buffer.delete(buffer.keys().next().value as string);
  }
  return p;
}

function applyResult(cards: Card[], e: LookupResultEvent): Card[] {
  return cards.map((c) =>
    c.id === e.dictId
      ? {
          ...c,
          status: e.status,
          error: e.error,
          ms: e.ms,
          entries: e.entries.map((x) => ({ headword: x.headword, ...prepareEntry(x.body, e.dictId) })),
        }
      : c,
  );
}

export const useLookup = create<LookupState>((set, get) => ({
  handle: null,
  cards: [],
  done: false,
  suggestions: [],
  expanded: {},
  error: null,

  search: async (query) => {
    const prev = get().handle;
    if (!query.trim()) {
      searchSeq++;
      if (prev) void cancelLookup(prev.requestId);
      get().clear();
      return;
    }
    if (prev && prev.query === query) return;
    const seq = ++searchSeq;
    try {
      const handle = await ipcLookup(query);
      if (seq !== searchSeq) {
        void cancelLookup(handle.requestId);
        return;
      }
      let cards: Card[] = handle.dicts.map((d) => ({
        id: d.id,
        name: d.name,
        status: "pending",
        entries: [],
        error: null,
        ms: null,
      }));
      const early = buffer.get(handle.requestId);
      buffer.delete(handle.requestId);
      for (const r of early?.results ?? []) cards = applyResult(cards, r);
      set({
        handle,
        cards,
        done: early?.done != null || handle.dicts.length === 0,
        suggestions: early?.done?.suggestions ?? [],
        expanded: {},
        error: null,
      });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  clear: () => set({ handle: null, cards: [], done: false, suggestions: [], expanded: {}, error: null }),

  setExpanded: (id, open) => set((s) => ({ expanded: { ...s.expanded, [id]: open } })),
}));

/** 订阅后端事件；返回取消订阅函数。整个窗口只调用一次。 */
export async function subscribeLookupEvents(): Promise<() => void> {
  const offResult = await onLookupResult((e) => {
    const { handle, cards } = useLookup.getState();
    if (handle?.requestId === e.requestId) {
      useLookup.setState({ cards: applyResult(cards, e) });
    } else {
      buffered(e.requestId).results.push(e);
    }
  });
  const offDone = await onLookupDone((e) => {
    if (useLookup.getState().handle?.requestId === e.requestId) {
      useLookup.setState({ done: true, suggestions: e.suggestions });
    } else {
      buffered(e.requestId).done = e;
    }
  });
  return () => {
    offResult();
    offDone();
  };
}

/** 卡片是否展开：用户手动设置优先，否则默认展开前 DEFAULT_EXPANDED 部命中的词典。 */
export function isExpanded(card: Card, hitRank: number, expanded: Record<string, boolean>): boolean {
  return expanded[card.id] ?? hitRank < DEFAULT_EXPANDED;
}
