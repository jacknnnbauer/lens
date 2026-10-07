// 生词本（SPEC 5.1、M4）。

import { invoke } from "@tauri-apps/api/core";
import type { ContextRow } from "./history";

export interface WordContext extends ContextRow {
  createdAt: number;
}

export interface Word {
  id: number;
  lemma: string;
  brief: string | null;
  /** 0 新词 · 1–4 学习中 · 5 已掌握 */
  familiarity: number;
  dueAt: number | null;
  createdAt: number;
  tags: string[];
  /** 新的在前 */
  contexts: WordContext[];
  lookups: number;
}

export type WordStatus = "new" | "learning" | "mastered";

export interface WordQuery {
  status?: WordStatus;
  tag?: string;
  search?: string;
  sort?: "added" | "alpha" | "familiarity";
  limit?: number;
  offset?: number;
}

export interface TagCount {
  name: string;
  count: number;
}

export interface WordbookStats {
  total: number;
  metThisWeek: number;
  frequentUnsaved: string[];
}

export function saveWord(query: string, opts: { brief?: string | null; contextId?: number | null; tags?: string[] } = {}): Promise<number> {
  return invoke<number>("save_word", { query, brief: opts.brief ?? null, contextId: opts.contextId ?? null, tags: opts.tags ?? null });
}

export function wordId(query: string): Promise<number | null> {
  return invoke<number | null>("word_id", { query });
}

export function listWords(q: WordQuery = {}): Promise<Word[]> {
  return invoke<Word[]>("list_words", { ...q });
}

export function updateWord(id: number, patch: { brief?: string; familiarity?: number; tags?: string[] }): Promise<void> {
  return invoke("update_word", { id, patch });
}

export function deleteWord(id: number): Promise<boolean> {
  return invoke<boolean>("delete_word", { id });
}

export function unlinkWordContext(wordId: number, contextId: number): Promise<void> {
  return invoke("unlink_word_context", { wordId, contextId });
}

export function listTags(): Promise<TagCount[]> {
  return invoke<TagCount[]>("list_tags");
}

export function wordbookStats(): Promise<WordbookStats> {
  return invoke<WordbookStats>("wordbook_stats");
}
