// 英文怎么说（Rust：commands/ai.rs · expression_ai.rs）

import { invoke } from "@tauri-apps/api/core";

export interface ExpressionItem {
  /** 简单 / 地道 / 母语级 / 幽默 / 含蓄 */
  style: string;
  en: string;
  /** 一句中文说明 */
  note: string;
}

export interface ExpressionResult {
  expression: { items: ExpressionItem[] };
  model: string;
  /** 缓存里取的：当时生成的时间；刚生成的为 null */
  cachedAt: number | null;
  costUsd: number;
}

/** 生成五种说法（先查缓存；force 时重新生成） */
export const expressionGenerate = (input: string, audience: string | null, force: boolean) =>
  invoke<ExpressionResult>("expression_generate", { input, audience, force });

/** 只查缓存，不联网 */
export const expressionCached = (input: string, audience: string | null) => invoke<ExpressionResult | null>("expression_cached", { input, audience });

// —— 更多说法（Rust：expression_entry.rs）——

export interface EntryItem {
  en: string;
  /** 中文语感 */
  zh: string;
  /** "" | 粗口 | 正式 */
  tag: string;
  /** 最常用的几条 */
  star: boolean;
  /** 「适合……」，没有为空 */
  when: string;
}

export interface EntryGroup {
  name: string;
  items: EntryItem[];
}

export interface DialogLine {
  who: string;
  en: string;
  zh: string;
}

export interface Entry {
  groups: EntryGroup[];
  dialogue: DialogLine[];
  /** 值得整块记的表达，可能为空 */
  tip: string;
}

export interface EntryResult {
  entry: Entry;
  model: string;
  cachedAt: number | null;
  costUsd: number;
}

/** 生成「更多说法」词条（先查缓存；force 时重新生成） */
export const expressionEntryGenerate = (input: string, force: boolean) => invoke<EntryResult>("expression_entry_generate", { input, force });

/** 只查缓存，不联网 */
export const expressionEntryCached = (input: string) => invoke<EntryResult | null>("expression_entry_cached", { input });
