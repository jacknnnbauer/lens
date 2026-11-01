// 搜索框里粘贴的是一句话还是要查的词组：一句话就铺开成可点的词，点哪个查哪个，并用这句话判断词义。
// 英文按字母串切词；中文没有空格，用浏览器自带的 Intl.Segmenter（ICU 词典）分词。

export interface SentenceWord {
  text: string;
  /** 在句中的 UTF-16 区间 */
  start: number;
  end: number;
}

// TS 的 ES2020 库里还没有 Intl.Segmenter，WebView2 和 Node 都有
interface Segment {
  segment: string;
  index: number;
  isWordLike?: boolean;
}
type Segmenter = { segment: (s: string) => Iterable<Segment> };
const SegmenterCtor = (Intl as unknown as { Segmenter?: new (locale: string, opts: { granularity: "word" }) => Segmenter }).Segmenter;
const zh: Segmenter | null = SegmenterCtor ? new SegmenterCtor("zh", { granularity: "word" }) : null;

const HAN = /\p{Script=Han}/u;

/** 句子切成词，带 UTF-16 区间：英文词（撇号、连字符算在词里）和中文词 */
export function splitWords(s: string): SentenceWord[] {
  const out: SentenceWord[] = [];
  const re = /[A-Za-z][A-Za-z'’-]*|\p{Script=Han}+/gu;
  for (let m = re.exec(s); m; m = re.exec(s)) {
    if (!HAN.test(m[0])) {
      out.push({ text: m[0], start: m.index, end: m.index + m[0].length });
      continue;
    }
    // 一段连续汉字再分词；没有分词器时退成一个字一个词
    const base = m.index;
    const parts: Segment[] = zh ? [...zh.segment(m[0])] : [...m[0]].map((c, i) => ({ segment: c, index: i, isWordLike: true }));
    for (const p of parts) {
      if (p.isWordLike === false) continue;
      out.push({ text: p.segment, start: base + p.index, end: base + p.index + p.segment.length });
    }
  }
  return out;
}

/**
 * 像一句话而不是词条：
 * - 英文 5 个词以上，或 3 个词以上并以句末标点结尾。「look forward to」「on the other hand」这类词组仍按词组查；
 * - 中文 10 个字以上，或 4 个字以上并带句中 / 句末标点。「精品店」「画蛇添足」仍按词查。
 */
export function isSentence(q: string): boolean {
  const t = q.trim();
  const en = splitWords(t).filter((w) => !HAN.test(w.text)).length;
  if (en >= 5 || (en >= 3 && /[.!?。！？]["'”’)]?$/.test(t))) return true;
  const han = (t.match(/\p{Script=Han}/gu) ?? []).length;
  return han >= 10 || (han >= 4 && /[，。！？；：、,.!?;]/.test(t));
}

/**
 * 句中选区扩到整词：光标落在词里取这个词（正好在两个词之间时取后一个，没有后一个就取前一个）；
 * 选了一段就扩到两端的词。选区里没有词时返回 null。
 */
export function wordSpan(s: string, start: number, end: number): { start: number; end: number } | null {
  const words = splitWords(s);
  const [a, b] = start <= end ? [start, end] : [end, start];
  const hit =
    a === b
      ? [words.find((w) => w.start <= a && a < w.end) ?? words.find((w) => w.end === a)].filter((w): w is SentenceWord => !!w)
      : words.filter((w) => w.end > a && w.start < b);
  if (hit.length === 0) return null;
  return { start: hit[0].start, end: hit[hit.length - 1].end };
}
