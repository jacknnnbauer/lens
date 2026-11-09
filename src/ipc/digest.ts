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
  /** 阅读建议：值得读原文 / 正常 / 可略读 / 可跳过，和理由 */
  advice: "read" | "normal" | "skim" | "skip";
  adviceWhy: string;
  /** 紧张程度 1–5（全书节奏图用） */
  tension: number;
  quotes: number[];
}

export interface DigestScene {
  id: number;
  chapter: number;
  title: string;
  /** 书里的原文（按开头句、结尾句取出的整段）；非公版书只有开头几句 */
  text: string;
  why: string;
  importance: number;
  zh: string;
  /** 前因后果 */
  context: string;
  clipped: boolean;
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
  diagrams: Diagrams;
}

/** 图里的一条连线；a → b（伏笔、因果的方向） */
export interface DiagramLink {
  a: string;
  b: string;
  kind: string;
  label: string;
}

export interface DiagramTree {
  nodes: Array<{ id: string; parent: string; name: string; role: string; special: boolean }>;
}

export interface DiagramGraph {
  nodes: Array<{ id: string; text: string; kind: string }>;
  links: DiagramLink[];
}

/** AI 挑的图示（chosen 按顺序排版）和各自的数据；没选的留空 */
export interface Diagrams {
  chosen: Array<{ kind: string; why: string }>;
  relations: { center: string; groups: Array<{ id: string; label: string }>; people: Array<{ id: string; name: string; en: string; group: string }>; links: DiagramLink[] };
  plot: { events: Array<{ id: string; chapter: number; text: string }>; links: DiagramLink[] };
  tree: DiagramTree;
  family: DiagramTree;
  route: { stops: Array<{ place: string; chapter: number; what: string }> };
  rhythm: { marks: Array<{ chapter: number; label: string }> };
  compare: { title: string; items: Array<{ name: string; en: string; chapter: number; badge: string; tone: string; text: string }> };
  causal: DiagramGraph;
  concepts: DiagramGraph;
}

export interface Digest {
  model: string;
  chapters: DigestChapter[];
  quotes: DigestQuote[];
  /** 名场面（最多十六段，按章节先后） */
  scenes: DigestScene[];
  synthesis: Synthesis;
  /** 作者卒年（维基百科）；去世超过 70 年算公版书，名场面放整段原文 */
  authorDied: number | null;
  publicDomain: boolean;
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
