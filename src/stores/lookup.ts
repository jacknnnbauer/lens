// 查词状态：当前查询、各词典卡片、未命中时的候选。
// 结果事件可能早于 lookup() 的返回到达，所以先按 requestId 缓冲，拿到句柄后再合并。

import { create } from "zustand";
import { prepareEntry, type PreparedEntry } from "../dict-render/prepare";
import { recordHistory } from "../ipc/history";
import { cancelLookup, lookup as ipcLookup, onLookupDone, onLookupResult } from "../ipc/lookup";
import { lookupOnline } from "../ipc/online";
import type { DictBrief, LookupDoneEvent, LookupHandle, LookupResultEvent, LookupStatus } from "../ipc/types";

const BUFFER_IDS = 4;

export interface SearchOptions {
  commit?: boolean;
  /** 划词捕获的语境句 id，随历史一起保存 */
  contextId?: number | null;
}

/** 请求了提交、但结果还没回来的查询 */
let pendingCommit: string | null = null;
/** 已写入历史的查询，避免同一次查询重复写 */
const committed = new Set<string>();
let currentContextId: number | null = null;

function flushCommit(state: { handle: LookupHandle | null; cards: Card[] }) {
  const h = state.handle;
  if (!h || pendingCommit !== h.requestId || committed.has(h.requestId)) return;
  if (!state.cards.some((c) => c.status === "hit")) return;
  pendingCommit = null;
  committed.add(h.requestId);
  recordHistory(h.query, currentContextId).catch((e) => console.warn("记录历史失败", e));
  // 侧栏「最近」只放确定查过、有结果的词，不放输入过程中防抖查的半截词
  const word = h.normalized || h.query.trim();
  const recent = useLookup.getState().recent;
  if (word) useLookup.setState({ recent: [word, ...recent.filter((w) => w !== word)].slice(0, RECENT_MAX) });
}
const RECENT_MAX = 12;

export interface CardEntry extends PreparedEntry {
  headword: string;
}

export interface Card {
  id: string;
  name: string;
  /** local / api（在线接口）/ web（网页模式） */
  source: DictBrief["source"];
  status: "pending" | LookupStatus;
  entries: CardEntry[];
  error: string | null;
  ms: number | null;
  /** 在线结果来自缓存时的获取时间 */
  cachedAt: number | null;
  stale: boolean;
  webUrl: string | null;
}

interface LookupState {
  handle: LookupHandle | null;
  /** 本次查询带的语境句 id（划词 / 窗口内查词）；手动输入的查询为 null */
  contextId: number | null;
  cards: Card[];
  done: boolean;
  suggestions: string[];
  /** 用户手动展开/折叠过的卡片 */
  expanded: Record<string, boolean>;
  error: string | null;
  /** 本次会话确定查过（记入历史）的词，侧栏「最近」用 */
  recent: string[];
  /** 最近一次「确定的」查询（Enter、点链接、划词…）；主窗口的后退 / 前进按它记步 */
  committed: { query: string; seq: number } | null;
  /** commit：这是一次「确定的」查询（Enter、点链接、划词…），有命中就记入历史 */
  search: (query: string, opts?: SearchOptions) => Promise<void>;
  /** 把当前查询记入历史（命中后才真正写入；同一次查询只写一次） */
  commit: () => void;
  /** 用同一个查询再查一次（切换场景配置组后） */
  rerun: () => Promise<void>;
  clear: () => void;
  setExpanded: (id: string, open: boolean) => void;
  /** 查询时机为「手动」的在线词典：用户点击后查询 */
  queryOnline: (id: string) => Promise<void>;
}

type Pending = { results: LookupResultEvent[]; done: LookupDoneEvent | null };
const buffer = new Map<string, Pending>();
/** 只认最后一次 search 的返回，防止较早的查询晚到后覆盖新结果 */
let searchSeq = 0;
let commitSeq = 0;

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
          cachedAt: e.cachedAt ?? null,
          stale: e.stale ?? false,
          webUrl: e.webUrl ?? null,
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
  recent: [],
  committed: null,
  contextId: null,

  commit: () => {
    const h = get().handle;
    if (!h) return;
    if (get().committed?.query !== h.query.trim()) set({ committed: { query: h.query.trim(), seq: ++commitSeq } });
    pendingCommit = h.requestId;
    flushCommit(get());
  },

  rerun: async () => {
    const q = get().handle?.query;
    if (!q) return;
    set({ handle: null });
    await get().search(q);
  },

  search: async (query, opts = {}) => {
    const prev = get().handle;
    if (!query.trim()) {
      searchSeq++;
      if (prev) void cancelLookup(prev.requestId);
      get().clear();
      return;
    }
    if (prev && prev.query === query) {
      if (opts.commit) get().commit();
      return;
    }
    const seq = ++searchSeq;
    if (opts.commit) set({ committed: { query: query.trim(), seq: ++commitSeq } });
    currentContextId = opts.contextId ?? null;
    set({ contextId: currentContextId });
    try {
      const handle = await ipcLookup(query);
      if (seq !== searchSeq) {
        void cancelLookup(handle.requestId);
        return;
      }
      let cards: Card[] = handle.dicts.map((d) => ({
        id: d.id,
        name: d.name,
        source: d.source,
        status: "pending",
        entries: [],
        error: null,
        ms: null,
        cachedAt: null,
        stale: false,
        webUrl: null,
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
      if (opts.commit) get().commit();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  clear: () => set({ handle: null, cards: [], done: false, suggestions: [], expanded: {}, error: null }),

  setExpanded: (id, open) => set((s) => ({ expanded: { ...s.expanded, [id]: open } })),

  queryOnline: async (id) => {
    const handle = get().handle;
    if (!handle) return;
    set((s) => ({ cards: s.cards.map((c) => (c.id === id ? { ...c, status: "pending", error: null } : c)), expanded: { ...s.expanded, [id]: true } }));
    try {
      const e = await lookupOnline(handle.requestId, id, handle.normalized || handle.query);
      if (get().handle?.requestId !== handle.requestId) return;
      set((s) => ({ cards: applyResult(s.cards, e) }));
      flushCommit(get());
    } catch (err) {
      if (get().handle?.requestId !== handle.requestId) return;
      set((s) => ({ cards: s.cards.map((c) => (c.id === id ? { ...c, status: "error", error: String(err) } : c)) }));
    }
  },
}));

/** 订阅后端事件；返回取消订阅函数。整个窗口只调用一次。 */
export async function subscribeLookupEvents(): Promise<() => void> {
  const offResult = await onLookupResult((e) => {
    const { handle, cards } = useLookup.getState();
    if (handle?.requestId === e.requestId) {
      useLookup.setState({ cards: applyResult(cards, e) });
      flushCommit(useLookup.getState());
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

export type ExpandMode = "first2" | "all" | "none";

/** 卡片是否展开：用户手动设置优先，否则按 settings.layout.cardsExpanded。 */
export function isExpanded(card: Card, hitRank: number, expanded: Record<string, boolean>, mode: ExpandMode = "first2"): boolean {
  // 手动查询 / 网页模式 / 出错的卡片默认展开，按钮和错误原因才看得见
  if (card.status === "manual" || card.status === "web" || card.status === "error") return expanded[card.id] ?? true;
  const byDefault = mode === "all" ? true : mode === "none" ? false : hitRank < 2;
  return expanded[card.id] ?? byDefault;
}
