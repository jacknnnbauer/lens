// 周报卡片排版：算出画哪几块、各块位置和要画的文字（纯计算，Canvas 绘制在 drawCard.ts）。
// 版式见 design/prototypes/weekly-card.html：1080×1620，四块——阅读时长、词汇量、新收生词、金句；没数据的块不画，下面的往上排。

import type { WeeklyReport } from "../ipc/weekly";
import { getLang, tr } from "../i18n";

export const W = 1080;
export const H = 1620;
export const PAD = 88;
export const CONTENT = W - PAD * 2;
/** 第一行（时长、词汇量）的顶 */
const ROW_TOP = 360;
const ROW_H = 330;
const WORDS_H = 210;
const GAP = 56;
/** 底部小字离卡片底边 */
export const FOOT_GAP = 72;
/** 没有金句时卡片按内容缩短，但不矮于这个高度 */
const MIN_H = 900;
/** 两栏时右栏的左边 */
export const RIGHT_X = 572;

export const FONT = {
  /** 大数字：用等高数字的字体（Georgia 的数字高低不齐） */
  number: `"Source Han Serif SC", "Noto Serif SC", Cambria, "Times New Roman", serif`,
  serif: `Georgia, "Source Han Serif SC", "Noto Serif SC", "SimSun", serif`,
  sans: `"Microsoft YaHei", "PingFang SC", sans-serif`,
};
export const QUOTE_FONT = `44px ${FONT.serif}`;
export const NOTE_FONT = `30px ${FONT.serif}`;
export const WORDS_FONT = `italic 30px ${FONT.serif}`;
export const QUOTE_LINE = 66;
export const NOTE_LINE = 48;
/** 金句框内文字的宽度（左边框 8 + 左右内边距 56） */
export const QUOTE_TEXT_W = CONTENT - 8 - 112;

export type Measure = (text: string, font: string) => number;

export interface CardLayout {
  /** 卡片高度：有金句时 1620，没有时按内容缩短 */
  height: number;
  time?: { top: number; text: string; daySeconds: number[]; sub: string };
  vocab?: { top: number; x: number; value: number; delta: number | null; previous: number | null; points: number[] };
  words?: { top: number; count: number; samples: string };
  quote?: { top: number; height: number; lines: string[]; noteLines: string[]; src: string };
}

/** 3 小时 20 分 / 45 分钟 / 2 小时 */
export function formatDuration(seconds: number): string {
  const m = Math.round(seconds / 60);
  const h = Math.floor(m / 60);
  const r = m % 60;
  if (h === 0) return tr("{m} 分钟", { m: r });
  return r === 0 ? tr("{h} 小时", { h }) : tr("{h} 小时 {m} 分", { h, m: r });
}

function wordsText(n: number): string {
  // 「万」只有中文用；英文直接写全数
  return n >= 10_000 && getLang() === "zh" ? tr("约 {n} 万词", { n: (n / 10_000).toFixed(1) }) : tr("约 {n} 词", { n: n.toLocaleString() });
}

/** 切成能换行的小块：英文按词（带后面的空格）、中文和其他字符一个一个 */
function tokens(text: string): string[] {
  return text.match(/[A-Za-z0-9'’\-.,;:!?"“”()]+\s*|\s+|./gu) ?? [];
}

/**
 * 按宽度换行：英文整词、中文按字；超过 maxLines 时最后一行末尾加「…」。
 * 一个词比整行还宽时按字母硬断。
 */
export function wrapText(text: string, maxWidth: number, measure: (s: string) => number, maxLines: number): string[] {
  const lines: string[] = [];
  let cur = "";
  const push = () => {
    lines.push(cur.trimEnd());
    cur = "";
  };
  for (const tok of tokens(text.replace(/\s+/g, " ").trim())) {
    if (measure(cur + tok.trimEnd()) <= maxWidth) {
      cur += tok;
      continue;
    }
    if (cur.trim()) push();
    if (measure(tok.trimEnd()) <= maxWidth) {
      cur = tok.trimStart();
      continue;
    }
    for (const ch of tok) {
      if (measure(cur + ch) > maxWidth && cur) push();
      cur += ch;
    }
  }
  if (cur.trim()) push();
  if (lines.length <= maxLines) return lines;
  const out = lines.slice(0, maxLines);
  let last = out[maxLines - 1];
  while (last && measure(`${last}…`) > maxWidth) last = [...last].slice(0, -1).join("");
  out[maxLines - 1] = `${last.trimEnd()}…`;
  return out;
}

/** 本周一到周日，每天的秒数 */
function sevenDays(report: WeeklyReport, fromDay: string): number[] {
  const [y, m, d] = fromDay.split("-").map(Number);
  return Array.from({ length: 7 }, (_, i) => {
    const day = new Date(y, m - 1, d + i);
    const key = `${day.getFullYear()}-${String(day.getMonth() + 1).padStart(2, "0")}-${String(day.getDate()).padStart(2, "0")}`;
    return report.days.find((x) => x.day === key)?.seconds ?? 0;
  });
}

/** 四块都没有时返回 null（不出卡片） */
export function layoutCard(report: WeeklyReport, quoteIndex: number, measure: Measure, fromDay: string): CardLayout | null {
  const out: CardLayout = { height: H };
  let y = ROW_TOP;

  const hasTime = report.readSeconds >= 60;
  const hasVocab = !!report.vocab;
  if (hasTime) {
    const daySeconds = sevenDays(report, fromDay);
    const readDays = daySeconds.filter((s) => s > 0).length;
    const sub = [tr("读了 {n} 天", { n: readDays }), report.readWords > 0 ? wordsText(report.readWords) : null].filter(Boolean).join(" · ");
    out.time = { top: y, text: formatDuration(report.readSeconds), daySeconds, sub };
  }
  if (report.vocab) {
    const { value, previous, points } = report.vocab;
    // 折线从上周的值画起：本周只有一个快照时也是一条线
    const line = [...(previous === null ? [] : [previous]), ...points.map(([, v]) => v)];
    out.vocab = { top: y, x: hasTime ? RIGHT_X : PAD, value, delta: previous === null ? null : value - previous, previous, points: line };
  }
  if (hasTime || hasVocab) y += ROW_H + GAP;

  if (report.newWords > 0) {
    const samples = wrapText(report.newWordSamples.join(" · "), CONTENT, (s) => measure(s, WORDS_FONT), 1)[0] ?? "";
    out.words = { top: y, count: report.newWords, samples };
    y += WORDS_H + GAP;
  }

  if (report.quotes.length > 0) {
    const q = report.quotes[((quoteIndex % report.quotes.length) + report.quotes.length) % report.quotes.length];
    const lines = wrapText(`“${q.quote.trim()}”`, QUOTE_TEXT_W, (s) => measure(s, QUOTE_FONT), 6);
    const noteLines = q.note ? wrapText(q.note, QUOTE_TEXT_W, (s) => measure(s, NOTE_FONT), 3) : [];
    const src = `— ${q.title}${q.chapter ? ` · ${q.chapter}` : ""}`;
    const bottom = H - FOOT_GAP - 70;
    out.quote = { top: y, height: Math.max(0, bottom - y), lines, noteLines, src };
  } else {
    out.height = Math.max(MIN_H, y - GAP + 150);
  }

  return out.time || out.vocab || out.words || out.quote ? out : null;
}
