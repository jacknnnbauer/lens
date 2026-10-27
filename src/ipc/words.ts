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
  /** 下次复习（毫秒时间戳）；没开始复习为 null */
  dueAt: number | null;
  /** 复习次数 */
  reps: number;
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
  sort?: "added" | "alpha" | "familiarity" | "due";
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

export interface ReviewCounts {
  /** 到期的复习（含学习中） */
  due: number;
  /** 今天还可以引入的新词 */
  new: number;
  doneToday: number;
}

export interface ReviewItem {
  word: Word;
  /** 评分 1..4 各自的下次到期时间 */
  nextDue: [number, number, number, number];
}

export function reviewCounts(dayStart: number): Promise<ReviewCounts> {
  return invoke<ReviewCounts>("review_counts", { dayStart });
}

export function reviewQueue(dayStart: number, limit?: number): Promise<ReviewItem[]> {
  return invoke<ReviewItem[]>("review_queue", { dayStart, limit: limit ?? null });
}

/** 1 忘了 · 2 模糊 · 3 记得 · 4 简单；返回下次到期时间 */
export function reviewRate(id: number, rating: number): Promise<number> {
  return invoke<number>("review_rate", { id, rating });
}

/** 导出生词本（按筛选）：anki = Anki 可直接导入的 .txt，csv = Excel 能打开的 CSV。返回导出的词数 */
export function exportWordbook(path: string, format: "anki" | "csv", q: { status?: WordStatus; tag?: string; search?: string } = {}): Promise<number> {
  return invoke<number>("export_wordbook", { path, format, status: q.status ?? null, tag: q.tag ?? null, search: q.search || null });
}

export interface AnkiOptions {
  deck: string;
  /** 识记卡（单词 → 释义） */
  recall: boolean;
  /** 语境卡（挖空的原句 → 单词） */
  context: boolean;
  /** 从本地词典取发音 */
  audio: boolean;
  accent: "us" | "uk";
}

export interface AnkiExportResult {
  words: number;
  cards: number;
  /** 带上了发音的词数 */
  audio: number;
}

/** 导出 Anki 牌组包（.apkg，按筛选）：笔记类型、卡片模板、发音都在里面，双击就能导入 */
export function exportAnkiPackage(path: string, options: AnkiOptions, q: { status?: WordStatus; tag?: string; search?: string } = {}): Promise<AnkiExportResult> {
  return invoke<AnkiExportResult>("export_anki_package", { path, options, status: q.status ?? null, tag: q.tag ?? null, search: q.search || null });
}
