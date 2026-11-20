// 长难句拆解（Rust：commands/ai.rs · sentence_ai.rs）

import { invoke } from "@tauri-apps/api/core";

export interface Breakdown {
  translation: string;
  /** 主干：原文片段和成分（主语、谓语…） */
  skeleton: { text: string; role: string }[];
  /** 从句、短语：原文片段、类型、修饰谁、中文 */
  parts: { text: string; kind: string; modifies: string; zh: string }[];
  points: { title: string; explain: string }[];
}

export interface BreakdownResult {
  breakdown: Breakdown;
  model: string;
  /** 缓存里取的：当时拆的时间；刚拆的为 null */
  cachedAt: number | null;
  costUsd: number;
}

export const sentenceBreakdown = (sentence: string, before: string | null, after: string | null, force: boolean) =>
  invoke<BreakdownResult>("sentence_breakdown", { sentence, before, after, force });
