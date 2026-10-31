// 搜索框里粘贴的是一句话还是要查的词组：一句话就铺开成可点的词，点哪个查哪个，并用这句话判断词义。

export interface SentenceWord {
  text: string;
  /** 在句中的 UTF-16 区间 */
  start: number;
  end: number;
}

/** 句子按英文词切开，带 UTF-16 区间 */
export function splitWords(s: string): SentenceWord[] {
  const out: SentenceWord[] = [];
  const re = /[A-Za-z][A-Za-z'’-]*/g;
  for (let m = re.exec(s); m; m = re.exec(s)) out.push({ text: m[0], start: m.index, end: m.index + m[0].length });
  return out;
}

/**
 * 像一句话而不是词条：5 个词以上，或 3 个词以上并以句末标点结尾。
 * 「look forward to」「on the other hand」这类词组仍按词组查。
 */
export function isSentence(q: string): boolean {
  const t = q.trim();
  const n = splitWords(t).length;
  return n >= 5 || (n >= 3 && /[.!?。！？]["'”’)]?$/.test(t));
}

/**
 * 句中选区扩到整词：光标落在词里（或紧挨词边）取这个词；选了一段就扩到两端的词。
 * 选区里没有词时返回 null。
 */
export function wordSpan(s: string, start: number, end: number): { start: number; end: number } | null {
  const words = splitWords(s);
  const [a, b] = start <= end ? [start, end] : [end, start];
  const hit = a === b ? words.filter((w) => w.start <= a && a <= w.end).slice(0, 1) : words.filter((w) => w.end > a && w.start < b);
  if (hit.length === 0) return null;
  return { start: hit[0].start, end: hit[hit.length - 1].end };
}
