// 查词历史（SPEC 5.1、M4）。

import { invoke } from "@tauri-apps/api/core";

export interface ContextRow {
  id: number;
  sentence: string;
  hlStart: number;
  hlEnd: number;
  app: string | null;
  windowTitle: string | null;
  url: string | null;
}

export interface HistoryItem {
  id: number;
  query: string;
  lemma: string;
  /** main（主窗口）| popup（划词弹窗） */
  source: string;
  createdAt: number;
  context: ContextRow | null;
  /** 这个词一共查过几次 */
  lookups: number;
}

export interface HistoryQuery {
  from?: number;
  to?: number;
  search?: string;
  limit?: number;
  offset?: number;
}

export function recordHistory(query: string, contextId?: number | null): Promise<number | null> {
  return invoke<number | null>("record_history", { query, contextId: contextId ?? null });
}

export function listHistory(q: HistoryQuery = {}): Promise<HistoryItem[]> {
  return invoke<HistoryItem[]>("list_history", { ...q });
}

export function deleteHistory(ids: number[]): Promise<number> {
  return invoke<number>("delete_history", { ids });
}

export function clearHistory(before?: number): Promise<number> {
  return invoke<number>("clear_history", { before: before ?? null });
}

export function lookupCount(query: string): Promise<number> {
  return invoke<number>("lookup_count", { query });
}
