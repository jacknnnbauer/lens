// 批注定位：存「原文 + 前后各 32 个字」，正文改过、书重新导入后按它重新找到位置（不依赖界面，单独测试）。

export interface Anchor {
  quote: string;
  prefix: string;
  suffix: string;
}

const CONTEXT = 32;

export function makeAnchor(text: string, start: number, end: number): Anchor {
  return {
    quote: text.slice(start, end),
    prefix: text.slice(Math.max(0, start - CONTEXT), start),
    suffix: text.slice(end, end + CONTEXT),
  };
}

/** 从贴着的地方往外数，有几个字相同 */
function sameFromEnd(a: string, b: string): number {
  let n = 0;
  while (n < a.length && n < b.length && a[a.length - 1 - n] === b[b.length - 1 - n]) n++;
  return n;
}

function sameFromStart(a: string, b: string): number {
  let n = 0;
  while (n < a.length && n < b.length && a[n] === b[n]) n++;
  return n;
}

/** 在正文里找回这条批注；原文找不到返回 null */
export function locate(text: string, a: Anchor): [number, number] | null {
  if (!a.quote) return null;
  const whole = text.indexOf(a.prefix + a.quote + a.suffix);
  if (whole >= 0) {
    const s = whole + a.prefix.length;
    return [s, s + a.quote.length];
  }
  // 原文出现几处：前后文最像的那处（一样像时取前文更像的）
  let best: { at: number; score: number; pre: number } | null = null;
  for (let at = text.indexOf(a.quote); at >= 0; at = text.indexOf(a.quote, at + 1)) {
    const pre = sameFromEnd(text.slice(Math.max(0, at - a.prefix.length), at), a.prefix);
    const suf = sameFromStart(text.slice(at + a.quote.length, at + a.quote.length + a.suffix.length), a.suffix);
    const score = pre + suf;
    if (!best || score > best.score || (score === best.score && pre > best.pre)) best = { at, score, pre };
  }
  return best ? [best.at, best.at + a.quote.length] : null;
}
