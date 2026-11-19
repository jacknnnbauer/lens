// PDF 书的两种视图互相换算位置：重排的「第几章、章内读到哪（0–1）」⇄ 原版的「第几页」。
// pdfPages[i] = 第 i 章从原 PDF 第几页开始（从 1 起）；按页数均匀估算，够用来停在同一章附近。

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

/** 这一章占的页：[开始页, 下一章开始页) */
function span(pdfPages: number[], chapter: number, total: number): [number, number] {
  const start = pdfPages[chapter] ?? 1;
  const end = pdfPages[chapter + 1] ?? total + 1;
  return [start, Math.max(start + 1, end)];
}

export function pageFor(pdfPages: number[], chapter: number, pos: number, total: number): number {
  if (pdfPages.length === 0) return 1;
  const ch = clamp(chapter, 0, pdfPages.length - 1);
  const [start, end] = span(pdfPages, ch, total);
  const p = chapter > ch ? total : Math.floor(start + clamp(pos, 0, 1) * (end - start));
  return clamp(p, 1, Math.max(1, total));
}

export function chapterFor(pdfPages: number[], page: number, total: number): { chapter: number; pos: number } {
  let chapter = 0;
  pdfPages.forEach((start, i) => {
    if (start <= page) chapter = i;
  });
  const [start, end] = span(pdfPages, chapter, total);
  return { chapter, pos: clamp((page - start) / (end - start), 0, 1) };
}
