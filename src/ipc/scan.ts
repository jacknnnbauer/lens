// 文章生词扫描（Rust：commands/scan.rs、scan.rs）。

import { invoke } from "@tauri-apps/api/core";

export type ScanStatus = "new" | "saved" | "known";

export interface ScanWord {
  lemma: string;
  /** 文中出现过的词形（小写） */
  forms: string[];
  count: number;
  rank: number | null;
  status: ScanStatus;
  /** 第一次出现的那句话，词在句中的 UTF-16 区间 */
  sentence: string | null;
  hlStart: number;
  hlEnd: number;
}

export interface ScanResult {
  words: ScanWord[];
  /** 文中要高亮的词：UTF-16 区间 + words 下标 */
  tokens: Array<{ start: number; end: number; word: number }>;
  total: number;
  unique: number;
  /** 认识的词次占比 */
  coverage: number;
  /** 排名用的词频表 */
  source: "coca" | "builtin";
}

export function scanText(text: string, level: number): Promise<ScanResult> {
  return invoke<ScanResult>("scan_text", { text, level });
}

export function setKnownWords(lemmas: string[], known: boolean): Promise<void> {
  return invoke("set_known_words", { lemmas, known });
}

export interface ScanSave {
  lemma: string;
  surface: string;
  sentence: string | null;
  hlStart: number;
  hlEnd: number;
  brief: string | null;
}

export function saveScannedWords(items: ScanSave[], title?: string, tags?: string[]): Promise<number> {
  return invoke<number>("save_scanned_words", { items, title: title || null, tags: tags ?? null });
}
