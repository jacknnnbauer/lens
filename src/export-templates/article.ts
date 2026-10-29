// 文章导出（阅读 › 导出）：前半是原文，生词标在正文上并编号；后半是词汇表（单词、文中原句、词典释义）。
// 三种版式：书页（上标编号 + 文末词汇表）、旁注（每段右边注出本段新出现的词）、卡片（荧光笔标注 + 卡片词汇表）。
// 和单词导出共用导出窗口、样式（主题配色）与纸张；词典正文同样放进声明式 Shadow DOM。

import { escapeHtml } from "../copy/convert";
import { BASE_CSS } from "../dict-render/DictBody";
import { tr } from "../i18n";
import { COPYRIGHT, isoDate, trimEntry, type ExportStyle } from "./template";

export type ArticleLayout = "book" | "margin" | "cards";

export interface ArticleWord {
  lemma: string;
  /** 文中第一次出现时的写法 */
  surface: string;
  status: "new" | "saved";
  pos: string | null;
  gloss: string | null;
  sentence: string | null;
  hlStart: number;
  hlEnd: number;
  /** 词典释义（已清洗的词条 HTML）；没选或没查到时为 null */
  entry: { name: string; html: string[] } | null;
}

export interface ArticleExportInput {
  title: string;
  /** 阅读视图里排好版、包好词的正文 HTML（span.lens-w 带 data-lemma） */
  articleHtml: string;
  words: ArticleWord[];
  layout: ArticleLayout;
  style: ExportStyle;
  date: Date;
  options: { sentence: boolean; definition: boolean; examples: boolean; phonetics: boolean; meta: boolean };
  stats: { total: number; coverage: number };
}

const INK_CSS = `*, *::before, *::after, ::marker { color: var(--text) !important; background: transparent !important; border-color: var(--line) !important;
  box-shadow: none !important; text-shadow: none !important; }
img { filter: grayscale(1); }`;

// 旁注版里一段话算一行：这些顶层元素各自带一栏旁注
const BLOCKS = new Set(["P", "LI", "BLOCKQUOTE", "H1", "H2", "H3", "H4", "H5", "H6", "PRE", "TABLE", "UL", "OL", "HR", "DIV", "SPAN"]);

/**
 * 正文标注：要导出的词包成 `span.aw`（每处都标），第一次出现的地方加上标编号；其余词拆回纯文字。
 * 返回标好的 HTML 和每个顶层块里第一次出现的词（旁注用）。
 */
export function markArticle(
  html: string,
  numbers: Map<string, number>,
  title = "",
): { html: string; blocks: Array<{ html: string; first: number[] }> } {
  const doc = new DOMParser().parseFromString(`<div>${html}</div>`, "text/html");
  const root = doc.body.firstElementChild as HTMLElement;
  // 正文开头的一级标题和文章标题相同时去掉：标题统一放在卷首
  const h1 = root.firstElementChild;
  if (title && h1?.tagName === "H1" && h1.textContent?.trim() === title.trim()) h1.remove();
  const seen = new Set<number>();
  const blocks: Array<{ html: string; first: number[] }> = [];
  for (const block of Array.from(root.childNodes)) {
    const first: number[] = [];
    // DOMParser 出来的文档没有 defaultView，按 nodeType 判断元素
    const el = block.nodeType === 1 ? (block as HTMLElement) : null;
    const spans = el ? Array.from(el.querySelectorAll<HTMLElement>("span.lens-w")) : [];
    if (el?.matches("span.lens-w")) spans.unshift(el);
    for (const s of spans) {
      const n = s.dataset.lemma ? numbers.get(s.dataset.lemma) : undefined;
      if (n === undefined) {
        s.replaceWith(doc.createTextNode(s.textContent ?? ""));
        continue;
      }
      const mark = doc.createElement("span");
      mark.className = "aw";
      mark.textContent = s.textContent;
      s.replaceWith(mark);
      if (!seen.has(n)) {
        seen.add(n);
        first.push(n);
        const sup = doc.createElement("sup");
        sup.className = "an";
        sup.textContent = String(n);
        mark.after(sup);
      }
    }
    if (block.nodeType === 1 || (block.textContent ?? "").trim()) {
      const out = block.nodeType === 1 ? (block as HTMLElement).outerHTML : escapeHtml(block.textContent ?? "");
      if (block.nodeType === 1 && !BLOCKS.has((block as HTMLElement).tagName) && blocks.length) blocks[blocks.length - 1].html += out;
      else blocks.push({ html: out, first });
    }
  }
  // 包词时加的属性、正文里的标注状态都不带出去
  root.querySelectorAll("[data-start], [data-status], [data-lemma]").forEach((n) => {
    n.removeAttribute("data-start");
    n.removeAttribute("data-status");
    n.removeAttribute("data-lemma");
  });
  return { html: root.innerHTML, blocks };
}

function quote(w: ArticleWord): string {
  const s = w.sentence;
  if (!s) return "";
  return `${escapeHtml(s.slice(0, w.hlStart))}<mark>${escapeHtml(s.slice(w.hlStart, w.hlEnd))}</mark>${escapeHtml(s.slice(w.hlEnd))}`;
}

function shadow(entryHtml: string, style: ExportStyle): string {
  const css = BASE_CSS + (style === "inkSaving" ? INK_CSS : "");
  return `<template shadowrootmode="open" shadowrootserializable><style>${css}</style>${entryHtml}</template>`;
}

function entryHtml(w: ArticleWord, input: ArticleExportInput): string {
  if (!input.options.definition || !w.entry) return "";
  const o = { phonetics: input.options.phonetics, examples: input.options.examples, context: false, sourceDate: false };
  const bodies = w.entry.html.map((h) => `<div class="dict-body">${shadow(trimEntry(h, o), input.style)}</div>`).join("");
  return `<div class="gl-def"><div class="gl-dict">${escapeHtml(w.entry.name)}</div>${bodies}</div>`;
}

function wordHead(w: ArticleWord, n: number): string {
  const surface = w.surface && w.surface.toLowerCase() !== w.lemma.toLowerCase() ? `<span class="gl-form">${escapeHtml(w.surface)}</span>` : "";
  const tag = w.status === "saved" ? `<span class="gl-tag">${tr("已收藏")}</span>` : "";
  return `<div class="gl-head"><span class="gl-no">${n}</span><strong>${escapeHtml(w.lemma)}</strong>${w.pos ? `<em>${escapeHtml(w.pos)}</em>` : ""}${surface}${tag}</div>`;
}

function glossary(input: ArticleExportInput): string {
  if (input.words.length === 0) return "";
  const items = input.words
    .map((w, i) => {
      const n = i + 1;
      const gloss = w.gloss ? `<p class="gl-zh">${escapeHtml(w.gloss)}</p>` : "";
      const q = input.options.sentence && w.sentence ? `<blockquote class="gl-quote">${quote(w)}</blockquote>` : "";
      return `<section class="gl-item" id="g-${n}">${wordHead(w, n)}${gloss}${q}${entryHtml(w, input)}</section>`;
    })
    .join("");
  return `<section class="glossary"><h2>${tr("词汇表")}<small>${tr("{n} 个词", { n: input.words.length })}</small></h2><div class="gl-list">${items}</div></section>`;
}

function marginNotes(first: number[], input: ArticleExportInput): string {
  if (first.length === 0) return "";
  return first
    .map((n) => {
      const w = input.words[n - 1];
      if (!w) return "";
      return `<div class="mn"><span class="mn-no">${n}</span><strong>${escapeHtml(w.lemma)}</strong>${w.pos ? ` <em>${escapeHtml(w.pos)}</em>` : ""}${w.gloss ? `<span class="mn-zh">${escapeHtml(w.gloss)}</span>` : ""}</div>`;
    })
    .join("");
}

/** 文章导出文档的 <body> 内容 */
export function buildArticleBody(input: ArticleExportInput): string {
  const numbers = new Map(input.words.map((w, i) => [w.lemma, i + 1]));
  const marked = markArticle(input.articleHtml, numbers, input.title);
  const counts = { new: input.words.filter((w) => w.status === "new").length, saved: input.words.filter((w) => w.status === "saved").length };
  const meta = input.options.meta
    ? `<p class="ar-meta">${[
        tr("{n} 词", { n: input.stats.total }),
        tr("标注 {n} 个词", { n: input.words.length }),
        counts.saved ? tr("其中已收藏 {n} 个", { n: counts.saved }) : "",
        tr("认识 {pct}%", { pct: Math.round(input.stats.coverage * 100) }),
        `${tr("Lens 词镜")} · ${isoDate(input.date)}`,
      ]
        .filter(Boolean)
        .join(" · ")}</p>`
    : "";
  const body =
    input.layout === "margin"
      ? marked.blocks.map((b) => `<div class="mrow"><div class="mtext">${b.html}</div><aside class="mnote">${marginNotes(b.first, input)}</aside></div>`).join("")
      : marked.html;
  return `<main class="lens-export lens-article" data-layout="${input.layout}" data-style="${input.style === "inkSaving" ? "ink" : input.style.startsWith("theme:") ? "theme" : input.style}"><header class="ar-head"><div class="ar-kicker">${tr("阅读笔记")}</div><h1>${escapeHtml(input.title)}</h1>${meta}</header><article class="ar-body">${body}</article>${glossary(input)}<footer class="copyright">${tr(COPYRIGHT)}</footer></main>`;
}
