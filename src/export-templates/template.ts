// 单词导出模板（SPEC 11、画板 08）：卡片式 / 文档式。PDF 与 HTML 共用同一份文档。
// 每部词典的正文放在声明式 Shadow DOM 里（<template shadowrootmode>），词典 CSS 只作用于自己的卡片。

import { escapeHtml, isExample, isPhonetic } from "../copy/convert";
import { cleanTitle } from "../components/cleanTitle";
import { BASE_CSS } from "../dict-render/DictBody";
import { tr } from "../i18n";
import type { ContextRow } from "../ipc/history";

export type ExportLayout = "cards" | "document";
/** 纸本白底 / 跟随当前主题 / 黑白省墨 / 指定某套主题（内置或自定义） */
export type ExportStyle = "paperWhite" | "followTheme" | "inkSaving" | `theme:${string}`;
/** letter 只为兼容旧设置，界面只提供 A4 / A3 */
export type Paper = "a4" | "a3" | "letter";

/** 导出文档实际使用的主题 id */
export function exportTheme(style: ExportStyle, appliedTheme: string): string {
  if (style === "inkSaving") return "eink";
  if (style === "followTheme") return appliedTheme;
  if (style.startsWith("theme:")) return style.slice(6) || "paper";
  return "paper";
}

export const COPYRIGHT = "由 Lens 词镜导出。释义内容版权归各词典所有，仅供个人学习使用。";

export interface ExportOptions {
  phonetics: boolean;
  examples: boolean;
  context: boolean;
  sourceDate: boolean;
  /** 词典名称（标签行、卡片标题、「收录词典」行）；没有该字段时视为显示 */
  dictNames?: boolean;
}

export interface ExportCard {
  id: string;
  name: string;
  /** 在线词典标「在线」；本地词典不加标签 */
  online?: boolean;
  /** 已清洗、改写过的词条 HTML（prepare.ts 的输出） */
  entries: string[];
}

export interface ExportInput {
  word: string;
  date: Date;
  layout: ExportLayout;
  style: ExportStyle;
  options: ExportOptions;
  cards: ExportCard[];
  context: ContextRow | null;
}

// 黑白省墨：词典自带的彩色文字、底色、边框一律压成主题文字色（eink 主题下为纯黑），图片转灰度
const INK_CSS = `*, *::before, *::after, ::marker { color: var(--text) !important; background: transparent !important; border-color: var(--line) !important;
  box-shadow: none !important; text-shadow: none !important; }
img { filter: grayscale(1); }`;

export function isoDate(d: Date): string {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

/** 按选项裁剪词条：去掉例句、音标；去掉发音按钮与站内查词链接（导出文件里点不了）。 */
export function trimEntry(html: string, o: ExportOptions): string {
  const doc = new DOMParser().parseFromString(`<div>${html}</div>`, "text/html");
  const root = doc.body.firstElementChild as HTMLElement;
  root.querySelectorAll("[data-sound], script").forEach((n) => n.remove());
  root.querySelectorAll("*").forEach((n) => {
    if ((!o.examples && isExample(n)) || (!o.phonetics && isPhonetic(n))) n.remove();
  });
  root.querySelectorAll("a[data-entry]").forEach((a) => {
    const span = doc.createElement("span");
    span.append(...Array.from(a.childNodes));
    a.replaceWith(span);
  });
  // 有的词典在 span 上写 href，清洗时被改写成 lens 地址；离开 Lens 后无意义
  root.querySelectorAll("[href]:not(a, link)").forEach((n) => n.removeAttribute("href"));
  return root.innerHTML;
}

function shadow(entryHtml: string, style: ExportStyle): string {
  const css = BASE_CSS + (style === "inkSaving" ? INK_CSS : "");
  // shadowrootserializable：导出 HTML 时要把渲染后的 Shadow DOM 序列化回去
  return `<template shadowrootmode="open" shadowrootserializable><style>${css}</style>${entryHtml}</template>`;
}

function entriesHtml(card: ExportCard, input: ExportInput): string {
  return card.entries
    .map((e) => `<div class="entry"><div class="dict-body">${shadow(trimEntry(e, input.options), input.style)}</div></div>`)
    .join("");
}

/** 2026-10-22 14:05（本地时间，精确到分钟） */
export function isoMinute(ms: number): string {
  const d = new Date(ms);
  return `${isoDate(d)} ${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

// 语境句：句子在上；下面一行是来源（网页标题，有网址时可点）和读到这句、查这个词的时间
function contextHtml(input: ExportInput): string {
  const c = input.context;
  if (!c || !input.options.context) return "";
  const s = c.sentence;
  const quote = `${escapeHtml(s.slice(0, c.hlStart))}<mark>${escapeHtml(s.slice(c.hlStart, c.hlEnd))}</mark>${escapeHtml(s.slice(c.hlEnd))}`;
  let meta = "";
  if (input.options.sourceDate) {
    // 只写网页 / 窗口标题，不写 msedge 之类的进程名和浏览器尾巴
    const title = c.windowTitle ? escapeHtml(cleanTitle(c.windowTitle)) : "";
    const web = c.url && /^https?:/i.test(c.url) ? escapeHtml(c.url) : "";
    const src = web ? `<a href="${web}">${title || web}</a>` : title;
    const time = c.capturedAt ? `<time>${isoMinute(c.capturedAt)}</time>` : "";
    meta = [src, time].filter(Boolean).join(" · ");
  }
  return `<section class="context"><p>“${quote}”</p>${meta ? `<div class="source">${meta}</div>` : ""}</section>`;
}

// 语境句紧跟在词头下面（第二行），词典释义在后
function cardsLayout(input: ExportInput): string {
  const names = input.options.dictNames !== false;
  const meta = input.options.sourceDate ? `<span class="meta">${tr("Lens 词镜")} · ${isoDate(input.date)}</span>` : "";
  const tags = names && input.cards.length ? `<div class="dict-tags">${input.cards.map((c) => `<span>${escapeHtml(c.name)}</span>`).join("")}</div>` : "";
  const cards = input.cards
    .map(
      (c) =>
        `<article class="card">${names ? `<header><strong>${escapeHtml(c.name)}</strong>${c.online ? `<span>${tr("在线")}</span>` : ""}</header>` : ""}${entriesHtml(c, input)}</article>`,
    )
    .join("");
  return `<div class="wordbox"><h1>${escapeHtml(input.word)}</h1>${meta}</div>${contextHtml(input)}${tags}${cards}`;
}

function documentLayout(input: ExportInput): string {
  const row = input.options.sourceDate ? `<div class="row"><span>${tr("Lens 词镜 · 多词典释义")}</span><span>${isoDate(input.date)}</span></div>` : "";
  const names = input.options.dictNames !== false;
  const dicts = names && input.cards.length ? `<div class="dicts">${tr("收录词典：")}${input.cards.map((c) => escapeHtml(c.name)).join(" · ")}</div>` : "";
  const sections = input.cards
    .map(
      (c, i) =>
        `<section class="section">${
          names
            ? `<h2><span><span class="no">${String(i + 1).padStart(2, "0")}</span>${escapeHtml(c.name)}</span>${c.online ? `<span class="kind">${tr("在线")}</span>` : ""}</h2>`
            : ""
        }${entriesHtml(c, input)}</section>`,
    )
    .join("");
  return `<header class="doc-head">${row}<h1>${escapeHtml(input.word)}</h1>${dicts}</header>${contextHtml(input)}${sections}`;
}

function wordInner(input: ExportInput): string {
  return input.layout === "document" ? documentLayout(input) : cardsLayout(input);
}

function styleAttr(style: ExportStyle): string {
  if (style === "inkSaving") return "ink";
  return style.startsWith("theme:") ? "theme" : style;
}

/** 导出文档的 <body> 内容。 */
export function buildExportBody(input: ExportInput): string {
  return `<main class="lens-export" data-layout="${input.layout}" data-style="${styleAttr(input.style)}">${wordInner(input)}<footer class="copyright">${tr(COPYRIGHT)}</footer></main>`;
}

export interface VolumeWord {
  word: string;
  cards: ExportCard[];
  context: ContextRow | null;
}

export interface VolumeInput extends Omit<ExportInput, "word" | "cards" | "context"> {
  title: string;
  words: VolumeWord[];
}

/**
 * 批量导出合并模式的一卷（SPEC 12.3）：卷首标题 + 目录（页内链接）+ 每个词一节（另起一页）。
 * PDF 书签由打印时按标题层级生成：卷标题、目录、每个词的词头。
 */
export function buildVolumeBody(input: VolumeInput): string {
  const toc = input.words.map((w, i) => `<li><a href="#w-${i + 1}">${escapeHtml(w.word)}</a></li>`).join("");
  const sections = input.words
    .map((w, i) => `<section class="vol-word" id="w-${i + 1}">${wordInner({ ...input, word: w.word, cards: w.cards, context: w.context })}</section>`)
    .join("");
  const meta = input.options.sourceDate ? ` · ${isoDate(input.date)}` : "";
  return `<main class="lens-export lens-volume" data-layout="${input.layout}" data-style="${styleAttr(input.style)}"><header class="vol-head"><h1>${escapeHtml(input.title)}</h1><p>${tr("{n} 个词", { n: input.words.length })}${meta} · ${tr("Lens 词镜")}</p></header><nav class="toc" aria-label="${tr("目录")}"><h2>${tr("目录")}</h2><ol>${toc}</ol></nav>${sections}<footer class="copyright">${tr(COPYRIGHT)}</footer></main>`;
}

/** 文件名：去掉 Windows 不允许的字符、保留名、首尾空格和点，限长（SPEC 12 的规则子集）。 */
export function safeFileName(base: string): string {
  let s = base
    .normalize("NFKD")
    .replace(/[̀-ͯ]/g, "")
    .replace(/[\\/:*?"<>|\u0000-\u001f]/g, "_")
    .trim()
    .replace(/^[.\s]+|[.\s]+$/g, "");
  if (/^(con|prn|aux|nul|com[1-9]|lpt[1-9])$/i.test(s)) s = `${s}_`;
  return (s || "lens").slice(0, 120);
}
