// 词汇量测试和成长曲线（Rust：commands/vocab.rs）。

import { invoke } from "@tauri-apps/api/core";

export interface VocabAnswer {
  word: string;
  /** 词频段；null = 假词 */
  band: number | null;
  saidKnown: boolean;
  /** 释义抽查结果；null = 没抽查 */
  check: boolean | null;
}

export type VocabNext =
  | { kind: "word"; word: string; band: number | null; index: number; total: number }
  | { kind: "check"; word: string; options: [string, string, string]; correct: number; index: number; total: number }
  | { kind: "done" };

export interface VocabEstimate {
  value: number;
  low: number;
  high: number;
  capped: boolean;
  optimistic: boolean;
  /** 各词频段认识的比例（0–1） */
  profile: { from: number; to: number; known: number }[];
  stats: { real: number; realKnown: number; fakes: number; fakesCaught: number; checks: number; checksRight: number };
}

export interface VocabTestPoint {
  day: string;
  takenAt: number;
  value: number;
  low: number;
  high: number;
}

export interface VocabDayPoint {
  day: string;
  value: number;
}

export interface VocabSummary {
  current: number;
  gained: number;
  monthGain: number;
  lastTest: VocabTestPoint;
  tests: VocabTestPoint[];
  days: VocabDayPoint[];
}

/** 本地日期 YYYY-MM-DD */
export function localDay(d: Date = new Date()): string {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

export const vocabTestNext = (seed: number, answers: VocabAnswer[]) => invoke<VocabNext>("vocab_test_next", { seed, answers });
export const vocabTestFinish = (seed: number, answers: VocabAnswer[]) => invoke<VocabEstimate>("vocab_test_finish", { seed, answers, day: localDay() });
export const vocabRefresh = () => invoke<number | null>("vocab_refresh", { day: localDay() });
export function vocabSummary(): Promise<VocabSummary | null> {
  const now = new Date();
  return invoke("vocab_summary", { monthStartDay: localDay(new Date(now.getFullYear(), now.getMonth(), 1)) });
}
