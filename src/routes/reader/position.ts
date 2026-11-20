// 读到哪里：按本章纯文本算（屏幕顶上第一个词的起点 ÷ 全章字数），不按滚动高度算。
// 长章节会跳过屏幕外段落的排版（content-visibility），那时滚动高度只是估计，按它记位置会漂。

import type { WordIndex } from "./markdown";

/** 屏幕顶 top 处第一个露出来的词的位置占比（0–1）；spans 按正文先后排 */
export function readPos(spans: HTMLElement[], textLen: number, top: number): number {
  if (spans.length === 0 || textLen <= 0) return 0;
  let lo = 0;
  let hi = spans.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (spans[mid].getBoundingClientRect().bottom > top) hi = mid;
    else lo = mid + 1;
  }
  if (lo >= spans.length) return 1;
  return Math.min(1, Math.max(0, Number(spans[lo].dataset.start) / textLen));
}

/** 占比 pos 处的词：起点不早于 pos × 全章字数的第一个词（超出就取最后一个） */
export function spanAt(spans: HTMLElement[], pos: number, textLen: number): HTMLElement | null {
  if (spans.length === 0) return null;
  const target = pos * textLen - 0.5;
  let lo = 0;
  let hi = spans.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (Number(spans[mid].dataset.start) >= target) hi = mid;
    else lo = mid + 1;
  }
  return spans[Math.min(lo, spans.length - 1)];
}

/** 现在读到哪里（0–1）；还没排好正文时按滚动高度算 */
export function currentPos(ix: WordIndex | null, root: HTMLElement): number {
  if (!ix || ix.spans.length === 0) {
    const max = root.scrollHeight - root.clientHeight;
    return max > 0 ? Math.min(1, Math.max(0, root.scrollTop / max)) : 0;
  }
  return readPos(ix.spans, ix.plain.length, root.getBoundingClientRect().top + 1);
}

/** 滚到 pos 处的词（放在屏幕顶上）；pos 为 0 回到最上面 */
export function scrollToPos(ix: WordIndex | null, root: HTMLElement, pos: number) {
  const span = pos > 0 && ix ? spanAt(ix.spans, pos, ix.plain.length) : null;
  if (!span) {
    root.scrollTo({ top: pos > 0 ? pos * Math.max(0, root.scrollHeight - root.clientHeight) : 0 });
    return;
  }
  root.scrollTo({ top: root.scrollTop + span.getBoundingClientRect().top - root.getBoundingClientRect().top });
}
