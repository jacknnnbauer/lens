// 单词导出模板（SPEC 11、画板 08）：卡片式 / 文档式。PDF 与 HTML 共用同一份文档。
// 每部词典的正文放在声明式 Shadow DOM 里（<template shadowrootmode>），词典 CSS 只作用于自己的卡片。

import { escapeHtml, isExample, isPhonetic } from "../copy/convert";
import { BASE_CSS } from "../dict-render/DictBody";
import type { ContextRow } from "../ipc/history";

export type ExportLayout = "cards" | "document";
export type ExportStyle = "paperWhite" | "followTheme" | "inkSaving";
export type Paper = "a4" | "letter";

export const COPYRIGHT = "由 Lens 词镜导出。释义内容版权归各词典所有，仅供个人学习使用。";

export interface ExportOptions {
  phonetics: boolean;
  examples: boolean;
  context: boolean;
  sourceDate: boolean;
}

export interface ExportCard {
  id: string;
  name: string;
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

/** 省墨样式：词典自带的底色、阴影一律去掉 */
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

function contextHtml(input: ExportInput, label: string): string {
  const c = input.context;
  if (!c || !input.options.context) return "";
  const s = c.sentence;
  const quote = `${escapeHtml(s.slice(0, c.hlStart))}<mark>${escapeHtml(s.slice(c.hlStart, c.hlEnd))}</mark>${escapeHtml(s.slice(c.hlEnd))}`;
  const src = [c.windowTitle, c.app?.replace(/\.exe$/i, "")].filter(Boolean).map((x) => escapeHtml(x as string));
  const url = c.url && /^https?:/i.test(c.url) ? ` · <a href="${escapeHtml(c.url)}">${escapeHtml(c.url)}</a>` : "";
  const meta = input.options.sourceDate && src.length ? ` · ${src.join(" · ")}${url}` : "";
  return `<section class="context"><div class="label">${label}${meta}</div><p>“${quote}”</p></section>`;
}

function cardsLayout(input: ExportInput): string {
  const meta = input.options.sourceDate ? `<span class="meta">Lens 词镜 · ${isoDate(input.date)}</span>` : "";
  const tags = input.cards.map((c) => `<span>${escapeHtml(c.name)}</span>`).join("");
  const cards = input.cards
    .map((c) => `<article class="card"><header><strong>${escapeHtml(c.name)}</strong><span>离线</span></header>${entriesHtml(c, input)}</article>`)
    .join("");
  return `<div class="wordbox"><h1>${escapeHtml(input.word)}</h1>${meta}</div><div class="dict-tags">${tags}</div>${cards}${contextHtml(input, "语境句")}`;
}

function documentLayout(input: ExportInput): string {
  const row = input.options.sourceDate ? `<div class="row"><span>Lens 词镜 · 多词典释义</span><span>${isoDate(input.date)}</span></div>` : "";
  const dicts = `<div class="dicts">收录词典：${input.cards.map((c) => escapeHtml(c.name)).join(" · ")}</div>`;
  const sections = input.cards
    .map(
      (c, i) =>
        `<section class="section"><h2><span><span class="no">${String(i + 1).padStart(2, "0")}</span>${escapeHtml(c.name)}</span><span class="kind">离线</span></h2>${entriesHtml(c, input)}</section>`,
    )
    .join("");
  return `<header class="doc-head">${row}<h1>${escapeHtml(input.word)}</h1>${dicts}</header>${sections}${contextHtml(input, "我的语境")}`;
}

/** 导出文档的 <body> 内容。 */
export function buildExportBody(input: ExportInput): string {
  const inner = input.layout === "document" ? documentLayout(input) : cardsLayout(input);
  const style = input.style === "inkSaving" ? "ink" : input.style;
  return `<main class="lens-export" data-layout="${input.layout}" data-style="${style}">${inner}<footer class="copyright">${COPYRIGHT}</footer></main>`;
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
