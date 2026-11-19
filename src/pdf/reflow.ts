// PDF 重排：把 pdf.js 取出的「带位置的文字块」整理成可以排版阅读的章节（不依赖 pdf.js，单独测试）。
// 步骤：合字拆开 → 文字块拼成行 → 去掉页眉页脚和页码 → 双栏先左后右 → 接成段落（含行尾连字符）→ 认标题 → 分章。

import { tr } from "../i18n";

/** 一个文字块；y 从页顶往下量 */
export interface PdfItem {
  str: string;
  x: number;
  y: number;
  w: number;
  h: number;
  size: number;
}

export interface PdfPage {
  page: number;
  width: number;
  height: number;
  items: PdfItem[];
}

export interface PdfOutline {
  title: string;
  page: number;
  level: number;
}

export interface ReflowChapter {
  title: string;
  html: string;
  /** 这一章从原 PDF 第几页开始 */
  page: number;
}

export interface ReflowBook {
  chapters: ReflowChapter[];
  toc: { title: string; level: number; chapter: number }[];
}

/** 每章最多多少页（既没书签也没标题时按页数切） */
const PAGES_PER_CHAPTER = 20;
/** 页眉页脚只在页面上下这么宽的范围里找 */
const MARGIN_BAND = 0.1;

interface Line {
  page: number;
  /** 第几栏（0 左 / 1 右） */
  col: number;
  x0: number;
  x1: number;
  y: number;
  size: number;
  text: string;
}

const clean = (s: string) =>
  s
    .normalize("NFKC")
    .replace(/­/g, "")
    .replace(/[‐‑]/g, "-");

function median(values: number[]): number {
  if (values.length === 0) return 0;
  const v = [...values].sort((a, b) => a - b);
  return v[Math.floor(v.length / 2)];
}

/** 一页的文字块拼成行（同一高度、按 x 排） */
function pageLines(p: PdfPage): Line[] {
  const items = p.items.map((it) => ({ ...it, str: clean(it.str) })).filter((it) => it.str.trim() !== "");
  items.sort((a, b) => a.y - b.y || a.x - b.x);
  const rows: (typeof items)[] = [];
  for (const it of items) {
    // 和最近一行同高（容差半个字号），而且横向不重叠（双栏同一高度的两行要分开）
    const row = rows.find((r) => Math.abs(r[0].y - it.y) <= Math.max(r[0].size, it.size) * 0.5 && !r.some((o) => it.x < o.x + o.w - 1 && o.x < it.x + it.w - 1) && Math.min(...r.map((o) => Math.abs(it.x - (o.x + o.w)))) < it.size * 3);
    if (row) row.push(it);
    else rows.push([it]);
  }
  return rows.map((r) => {
    r.sort((a, b) => a.x - b.x);
    let text = "";
    let end = -Infinity;
    for (const it of r) {
      const gap = it.x - end;
      if (text && gap > it.size * 0.15 && !/\s$/.test(text) && !/^\s/.test(it.str)) text += " ";
      text += it.str;
      end = it.x + it.w;
    }
    return {
      page: p.page,
      col: 0,
      x0: Math.min(...r.map((i) => i.x)),
      x1: Math.max(...r.map((i) => i.x + i.w)),
      y: median(r.map((i) => i.y)),
      size: Math.max(...r.map((i) => i.size)),
      text: text.replace(/\s+/g, " ").trim(),
    };
  });
}

/** 页眉页脚：在 ≥3 页的同一相对位置出现、去掉数字后相同的短行；只有数字的行（页码） */
function stripRunningHeads(pages: PdfPage[], lines: Line[][]): Line[][] {
  const height = new Map(pages.map((p) => [p.page, p.height]));
  const inBand = (l: Line) => {
    const r = l.y / (height.get(l.page) || 1);
    return r < MARGIN_BAND || r > 1 - MARGIN_BAND;
  };
  const key = (l: Line) => `${Math.round((l.y / (height.get(l.page) || 1)) * 50)}|${l.text.replace(/[\divxlcIVXLC]+$/g, "").replace(/\d+/g, "").trim().toLowerCase()}`;
  const seen = new Map<string, Set<number>>();
  for (const pl of lines)
    for (const l of pl.filter(inBand)) {
      const k = key(l);
      if (!seen.has(k)) seen.set(k, new Set());
      seen.get(k)!.add(l.page);
    }
  return lines.map((pl) =>
    pl.filter((l) => {
      if (!inBand(l)) return true;
      if (/^[\divxlc\s.\-–—|]+$/i.test(l.text)) return false;
      return (seen.get(key(l))?.size ?? 0) < 3;
    }),
  );
}

/** 双栏：中线两边各有几行、互不跨过中缝，就先读左栏再读右栏 */
function orderColumns(p: PdfPage, lines: Line[]): Line[] {
  const byY = (a: Line, b: Line) => a.y - b.y || a.x0 - b.x0;
  if (lines.length >= 4) {
    const mid = p.width / 2;
    const left = lines.filter((l) => l.x0 < mid);
    const right = lines.filter((l) => l.x0 >= mid);
    if (left.length >= 2 && right.length >= 2) {
      const leftEnd = Math.max(...left.map((l) => l.x1));
      const rightStart = Math.min(...right.map((l) => l.x0));
      if (rightStart - leftEnd > p.width * 0.02) {
        return [...left.sort(byY), ...right.sort(byY).map((l) => ({ ...l, col: 1 }))];
      }
    }
  }
  return [...lines].sort(byY);
}

/** 按阅读顺序排好的全部行 */
function readingOrder(pages: PdfPage[]): Line[] {
  const raw = pages.map(pageLines);
  const stripped = stripRunningHeads(pages, raw);
  return pages.flatMap((p, i) => orderColumns(p, stripped[i]));
}

/** 行尾连字符：上一行以「字母-」结尾、下一行以小写字母开头 → 拼起来的词（小写） */
function hyphenPair(prev: string, next: string): string | null {
  const a = prev.match(/([A-Za-z]+)-$/);
  const b = next.match(/^([a-z]+)/);
  return a && b ? (a[1] + b[1]).toLowerCase() : null;
}

/** 可能要去掉连字符的词，交给词表判断 */
export function hyphenCandidates(pages: PdfPage[]): string[] {
  const lines = readingOrder(pages);
  const out: string[] = [];
  for (let i = 1; i < lines.length; i++) {
    const w = hyphenPair(lines[i - 1].text, lines[i].text);
    if (w && !out.includes(w)) out.push(w);
  }
  return out;
}

/** 扫描版：一半以上的页几乎没有可取的文字 */
export function isScanned(pages: PdfPage[]): boolean {
  if (pages.length === 0) return true;
  const thin = pages.filter((p) => p.items.reduce((n, it) => n + it.str.replace(/\s/g, "").length, 0) < 50).length;
  return thin > pages.length / 2;
}

const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

interface Block {
  kind: "h" | "p";
  text: string;
  page: number;
  /** 段落跨页时，每一页从 text 的哪里开始（第一项就是 page, 0） */
  marks: [page: number, offset: number][];
}

/** 行接成段落和标题 */
function blocks(lines: Line[], isWord: (w: string) => boolean): Block[] {
  const body = median(lines.flatMap((l) => Array<number>(Math.max(1, Math.min(200, l.text.length))).fill(l.size)));
  const isHeading = (l: Line) => body > 0 && l.size >= body * 1.4 && l.text.split(" ").length <= 12 && /\p{L}/u.test(l.text);
  // 左右栏的边界取全书的（单页行少时也稳）：左边取靠前的 x0，右边取靠后的 x1
  const colKey = (l: Line) => `${l.page}|${l.col}`;
  const pct = (v: number[], q: number) => [...v].sort((a, b) => a - b)[Math.min(v.length - 1, Math.floor(v.length * q))] ?? 0;
  const bounds = new Map<number, { left: number; right: number }>();
  for (const col of [0, 1]) {
    const ls = lines.filter((l) => l.col === col && !isHeading(l));
    if (ls.length) bounds.set(col, { left: pct(ls.map((l) => l.x0), 0.1), right: pct(ls.map((l) => l.x1), 0.9) });
  }
  const gaps: number[] = [];
  for (let i = 1; i < lines.length; i++) {
    const a = lines[i - 1];
    const b = lines[i];
    if (colKey(a) === colKey(b) && !isHeading(a) && !isHeading(b) && b.y > a.y) gaps.push(b.y - a.y);
  }
  const lineGap = median(gaps);

  const out: Block[] = [];
  let cur: Block | null = null;
  let prev: Line | null = null;
  for (const l of lines) {
    if (isHeading(l)) {
      const last = out[out.length - 1];
      // 两行的大标题合成一个
      if (cur === null && last?.kind === "h" && prev && isHeading(prev) && prev.page === l.page) last.text += ` ${l.text}`;
      else {
        if (cur) out.push(cur);
        cur = null;
        out.push({ kind: "h", text: l.text, page: l.page, marks: [[l.page, 0]] });
      }
      prev = l;
      continue;
    }
    const b = bounds.get(l.col) ?? { left: l.x0, right: l.x1 };
    let newPara = cur === null || prev === null;
    if (!newPara && prev) {
      const sameCol = colKey(prev) === colKey(l);
      const gap = l.y - prev.y;
      const pb = bounds.get(prev.col) ?? b;
      const indent = l.x0 - b.left > l.size * 0.9;
      const prevShort = prev.x1 < pb.left + (pb.right - pb.left) * 0.7 && /[.!?:;"”’)]$/.test(prev.text);
      newPara = (sameCol && lineGap > 0 && gap > lineGap * 1.5) || indent || prevShort;
      // 换页处：上一页最后一行句子结束、这一行大写开头，也当新段（分不清时宁可多分一段）
      if (!newPara && prev.page !== l.page && /[.!?"”’)]$/.test(prev.text) && /^[\p{Lu}"“‘(]/u.test(l.text)) newPara = true;
    }
    if (newPara) {
      if (cur) out.push(cur);
      cur = { kind: "p", text: l.text, page: l.page, marks: [[l.page, 0]] };
    } else if (cur && prev) {
      const w = hyphenPair(cur.text, l.text);
      if (l.page !== prev.page) cur.marks.push([l.page, cur.text.length + (w ? 0 : 1)]);
      if (w) cur.text = isWord(w) ? cur.text.slice(0, -1) + l.text : cur.text + l.text;
      else cur.text += ` ${l.text}`;
    }
    prev = l;
  }
  if (cur) out.push(cur);
  return out;
}

function html(bs: Block[]): string {
  return bs.map((b) => (b.kind === "h" ? `<h2>${esc(b.text)}</h2>` : `<p>${esc(b.text)}</p>`)).join("\n");
}

export function reflow(pages: PdfPage[], outline: PdfOutline[], isWord: (w: string) => boolean): ReflowBook {
  const bs = blocks(readingOrder(pages), isWord);
  const lastPage = pages.length ? pages[pages.length - 1].page : 1;
  // 一段页码范围里的内容：跨页的段落按页拆开
  const chunk = (from: number, to: number): Block[] =>
    bs.flatMap((b) => {
      const keep = b.marks.map(([pg, off], i) => ({ pg, text: b.text.slice(off, b.marks[i + 1]?.[1] ?? b.text.length) })).filter((m) => m.pg >= from && m.pg < to);
      if (keep.length === 0) return [];
      return [{ ...b, page: keep[0].pg, text: keep.map((k) => k.text).join("").trim(), marks: [[keep[0].pg, 0]] }];
    });

  // 1. 有书签：按最高一级书签分章
  const marks = outline.filter((o) => o.title.trim() && o.page >= 1).sort((a, b) => a.page - b.page);
  if (marks.length > 0) {
    const top = Math.min(...marks.map((o) => o.level));
    const starts = marks.filter((o) => o.level === top).filter((o, i, a) => i === 0 || o.page !== a[i - 1].page);
    const chapters: ReflowChapter[] = [];
    const front = chunk(1, starts[0].page);
    if (front.length) chapters.push({ title: front.find((b) => b.kind === "h")?.text ?? tr("开头"), html: html(front), page: 1 });
    starts.forEach((s, i) => chapters.push({ title: s.title.trim(), html: html(chunk(s.page, starts[i + 1]?.page ?? Infinity)), page: s.page }));
    const toc = marks.map((m) => ({
      title: m.title.trim(),
      level: m.level - top,
      chapter: Math.max(0, chapters.reduce((at, c, i) => (c.page <= m.page ? i : at), 0)),
    }));
    return { chapters, toc };
  }

  // 2. 没书签：大字号标题开新章（至少要分出两章）
  const headingAt = bs.map((b, i) => (b.kind === "h" ? i : -1)).filter((i) => i >= 0);
  if (headingAt.length >= 2) {
    const chapters: ReflowChapter[] = [];
    if (headingAt[0] > 0) {
      const front = bs.slice(0, headingAt[0]);
      chapters.push({ title: tr("开头"), html: html(front), page: front[0].page });
    }
    headingAt.forEach((h, i) => {
      const part = bs.slice(h, headingAt[i + 1] ?? bs.length);
      chapters.push({ title: bs[h].text, html: html(part), page: bs[h].page });
    });
    return { chapters, toc: chapters.map((c, i) => ({ title: c.title, level: 0, chapter: i })) };
  }

  // 3. 都没有：每 20 页一章
  const chapters: ReflowChapter[] = [];
  for (let from = 1; from <= lastPage; from += PAGES_PER_CHAPTER) {
    const to = Math.min(lastPage, from + PAGES_PER_CHAPTER - 1);
    const part = chunk(from, to + 1);
    if (part.length) chapters.push({ title: tr("第 {a}–{b} 页", { a: from, b: to }), html: html(part), page: from });
  }
  return { chapters, toc: chapters.map((c, i) => ({ title: c.title, level: 0, chapter: i })) };
}
