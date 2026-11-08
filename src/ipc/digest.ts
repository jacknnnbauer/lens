// 精读本（全书预习包的「替代版」）：AI 通读全书写出的分析。后端 commands/digest.rs，每一步存成文件，中途停了下次接着做。

import { invoke } from "@tauri-apps/api/core";

export interface DigestPlan {
  provider: "Claude" | "DeepSeek";
  model: string;
  hasKey: boolean;
  /** 通读的段数；总步数 = 段数 + 4（综合） */
  chunks: number;
  steps: number;
  /** 已经做完的步 */
  done: number[];
  chapters: number;
  words: number;
  /** 估算的总花费和还要花的（美元） */
  costUsd: number;
  remainingUsd: number;
}

export interface DigestQuote {
  id: number;
  chapter: number;
  /** 书里的原文（逐字核对过） */
  text: string;
  zh: string;
  why: string;
}

export interface DigestChapter {
  idx: number;
  title: string;
  summary: string;
  kind: "plot" | "digression" | "other";
  events: string[];
  characters: string[];
  quotes: number[];
}

export interface Synthesis {
  oneLine: string;
  paragraph: string;
  talk: string;
  structure: Array<{ title: string; from: number; to: number; summary: string }>;
  characters: Array<{ name: string; role: string; arc: string; scenes: string; meaning: string; quote: number }>;
  relations: string;
  background: string;
  glossary: Array<{ term: string; zh: string; note: string }>;
  timeline: Array<{ chapter: number; event: string }>;
  motifs: Array<{ name: string; meaning: string; chapters: number[] }>;
  themes: Array<{ name: string; text: string; chapters: number[] }>;
  technique: string;
  reception: string;
  criticism: Array<{ claim: string; pro: string; con: string }>;
  origins: string;
  misconceptions: Array<{ wrong: string; right: string }>;
  topics: Array<{ topic: string; view: string; counter: string }>;
  golden: number[];
  further: Array<{ title: string; author: string; why: string }>;
  faq: Array<{ q: string; a: string; chapters: number[] }>;
  quiz: Array<{ q: string; a: string; chapter: number }>;
}

export interface Digest {
  model: string;
  chapters: DigestChapter[];
  quotes: DigestQuote[];
  synthesis: Synthesis;
  /** 模型给了、但在书里找不到而丢掉的原文条数 */
  dropped: number;
}

export function bookDigestPlan(id: number): Promise<DigestPlan> {
  return invoke<DigestPlan>("book_digest_plan", { id });
}

/** 做一步（做过的直接返回）；通读的几步可以并发，综合的四步要等通读全部做完 */
export function bookDigestStep(id: number, step: number): Promise<void> {
  return invoke<void>("book_digest_step", { id, step });
}

/** 全部做完时的精读本；没做完为 null */
export function bookDigest(id: number): Promise<Digest | null> {
  return invoke<Digest | null>("book_digest", { id });
}
