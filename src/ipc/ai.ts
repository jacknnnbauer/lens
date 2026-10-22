// COCA 词频表与 AI 双语例句（Rust：commands/ai.rs）。

import { invoke } from "@tauri-apps/api/core";

export interface CocaEntry {
  rank: number;
  lemma: string;
  /** 原表单字母词性：n v j r i c … */
  pos: string;
  freq: number;
  disp: number;
  /** S 口语 · F 小说 · M 杂志 · N 报纸 · A 学术（可多个，空格分隔） */
  register: string;
  synonyms: string[];
  /** adj / noun / verb / misc / all → 逗号分隔的搭配词，• 标出词头位置 */
  collocates: Record<string, string>;
}

export interface CocaStatus {
  loaded: boolean;
  entries: number;
}

export interface Example {
  en: string;
  zh: string;
  collocation: string;
}

export interface Examples {
  word: string;
  model: string;
  examples: Example[];
  cachedAt: number | null;
}

export const cocaStatus = () => invoke<CocaStatus>("coca_status");
export const cocaImport = (path: string) => invoke<CocaStatus>("coca_import", { path });
export const cocaLookup = (word: string) => invoke<CocaEntry[]>("coca_lookup", { word });

/** provider 省略时为 claude */
export const aiKeyStatus = (provider?: string) => invoke<boolean>("ai_key_status", { provider: provider ?? null });
/** 传 null 删除 */
export const aiSetKey = (provider: string, key: string | null) => invoke<boolean>("ai_set_key", { provider, key });

/** cachedOnly：只取缓存，不调用接口（查词时自动显示用，不花钱） */
export function aiExamples(word: string, opts: { brief?: string | null; force?: boolean; cachedOnly?: boolean } = {}): Promise<Examples | null> {
  return invoke<Examples | null>("ai_examples", { word, brief: opts.brief ?? null, force: !!opts.force, cachedOnly: !!opts.cachedOnly });
}

/** 搭配词展开：•gap → generation gap，boom• → boom generation */
export function expandCollocate(lemma: string, c: string): string {
  const s = c.trim();
  if (s.startsWith("•")) return `${lemma} ${s.slice(1).trim()}`;
  if (s.endsWith("•")) return `${s.slice(0, -1).trim()} ${lemma}`;
  return s;
}

export interface ModelUsage {
  model: string;
  calls: number;
  input: number;
  cached: number;
  output: number;
  costUsd: number;
}

export interface UsageSummary {
  models: ModelUsage[];
  total: ModelUsage;
  month: ModelUsage;
  since: number | null;
}

/** monthStart：本月 1 日 0 点（本地时间，毫秒） */
export const aiUsage = (monthStart: number) => invoke<UsageSummary>("ai_usage", { monthStart });
export const aiUsageClear = () => invoke<void>("ai_usage_clear");
