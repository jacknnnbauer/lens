// 词汇量测试界面的纯逻辑：键盘操作、作答记录、结果显示。

import { tr } from "../../i18n";
import type { VocabAnswer, VocabEstimate } from "../../ipc/vocabTest";

export type Action = { known: boolean } | { option: number | null };

export function keyToAction(key: string, kind: "word" | "check"): Action | null {
  if (kind === "word") {
    if (key === "ArrowLeft") return { known: false };
    if (key === "ArrowRight") return { known: true };
    return null;
  }
  if (key === "1" || key === "2" || key === "3") return { option: Number(key) - 1 };
  if (key === "4") return { option: null };
  return null;
}

export function withAnswer(answers: VocabAnswer[], q: { word: string; band: number | null }, known: boolean): VocabAnswer[] {
  return [...answers, { word: q.word, band: q.band, saidKnown: known, check: null }];
}

/** 抽查结果写回最后一道词题；`option` = null 表示选了「不确定」 */
export function withCheck(answers: VocabAnswer[], option: number | null, correct: number): VocabAnswer[] {
  const out = answers.slice();
  const last = out.length - 1;
  out[last] = { ...out[last], check: option === correct };
  return out;
}

const hundreds = (n: number) => (Math.round(n / 100) * 100).toLocaleString("en-US");

export function formatEstimate(e: VocabEstimate): string {
  if (e.capped) return `${e.value.toLocaleString("en-US")}+`;
  return tr("约 {v}（{low}–{high}）", { v: hundreds(e.value), low: hundreds(e.low), high: hundreds(e.high) });
}
