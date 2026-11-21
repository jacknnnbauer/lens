// 阅读周报（Rust：commands/weekly.rs · db/weekly.rs）

import { invoke } from "@tauri-apps/api/core";
import type { Week } from "../weekly/week";

export interface WeeklyQuote {
  quote: string;
  note: string | null;
  /** 书名 / 文章名 */
  title: string;
  /** 电子书章节名 */
  chapter: string | null;
  createdAt: number;
}

export interface WeeklyReport {
  /** 有阅读记录的那几天 */
  days: { day: string; seconds: number }[];
  readSeconds: number;
  readWords: number;
  vocab: { value: number; previous: number | null; points: [string, number][] } | null;
  newWords: number;
  newWordSamples: string[];
  quotes: WeeklyQuote[];
}

/** 记一次阅读：今天（本地日期）这篇读了几秒、往前读了多少词 */
export const readingLogAdd = (day: string, articleId: number, seconds: number, words: number) =>
  invoke<void>("reading_log_add", { day, articleId, seconds, words });

export const weeklyReport = (w: Week) => invoke<WeeklyReport>("weekly_report", { fromDay: w.fromDay, toDay: w.toDay, fromMs: w.fromMs, toMs: w.toMs });
