// 长难句拆解的选区：够不够长、会不会太长；以及前后各一句（发给模型判断指代）。

const WORD = /[A-Za-z]+(?:['’][A-Za-z]+)*/g;
// 常见缩写后的句点不算句末
const ABBR = /\b(?:Mr|Mrs|Ms|Dr|St|Jr|Sr|Prof|vs|etc|e\.g|i\.e)\.$/i;

/** 句末位置（句末标点后，含紧跟的引号括号）的列表 */
function sentenceEnds(text: string): number[] {
  const ends: number[] = [];
  const re = /[.!?。！？]["'”’)\]]*(?=\s|$)/g;
  for (let m = re.exec(text); m; m = re.exec(text)) {
    const end = m.index + m[0].length;
    if (ABBR.test(text.slice(Math.max(0, end - 6), m.index + 1))) continue;
    ends.push(end);
  }
  return ends;
}

/** ok：可以拆；short：不到 6 个词；long：超过 3 句或 600 个字符 */
export function breakdownFit(text: string): "ok" | "short" | "long" {
  const t = text.trim();
  if ((t.match(WORD)?.length ?? 0) < 6) return "short";
  if (t.length > 600 || sentenceEnds(t).length > 3) return "long";
  return "ok";
}

/** 选区本身，以及它前面一句、后面一句 */
export function contextAround(text: string, start: number, end: number): { sentence: string; before: string | null; after: string | null } {
  const ends = sentenceEnds(text);
  const prevEnd = [...ends].reverse().find((e) => e <= start) ?? 0;
  const prevStart = [...ends].reverse().find((e) => e < prevEnd) ?? 0;
  const before = text.slice(prevStart, prevEnd).trim() || null;
  const nextEnd = ends.find((e) => e > end) ?? text.length;
  const after = text.slice(end, nextEnd).trim() || null;
  return { sentence: text.slice(start, end).trim(), before, after };
}
