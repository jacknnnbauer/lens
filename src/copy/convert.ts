// 复制格式转换（SPEC 10）。输入为 prepare.ts 清洗、改写过的词条 HTML。
//
// 词典 HTML 千差万别，「义项 / 例句 / 音标」只能按常见 class 名识别，尽力而为：
// - 义项（sense/def/meaning…）→ Markdown 有序列表（统一写 `1.`，渲染器自动编号），去掉自带的数字
// - 例句（example/exa/eg/x…）→ 斜体子列表；includeExamples=false 时整段去掉
// - 音标（phon/pron/ipa…）→ 行内代码
// - 图片：只有 http(s) / data: 地址才保留为链接，lens:// 资源离开 Lens 就打不开，直接去掉
// - 发音按钮、页内锚点、站内查词链接只保留文字

import TurndownService from "turndown";
import { tr } from "../i18n";

export type CopyFormat = "md" | "html" | "text";

export interface CopyOptions {
  includeExamples: boolean;
  imagesAsLinks: boolean;
}

export interface CopySource {
  dict: string;
  headword: string;
}

const EXAMPLE_RE = /(^|[\s_-])(ex|exa|example|examples|eg|x|xexample|sentence|sample|sen|liju)([\s_-]|$)/i;
const SENSE_RE = /(^|[\s_-])(sense|sn|sn-g|def|defs|meaning|definition|sense-block)([\s_-]|$)/i;
const PHONETIC_RE = /(^|[\s_-])(phon|phonetic|pron|ipa|pronunciation|yinbiao)([\s_-]|$)/i;
const BLOCK_TAGS = new Set([
  "ADDRESS", "ARTICLE", "ASIDE", "BLOCKQUOTE", "DD", "DIV", "DL", "DT", "FIGURE", "FOOTER", "H1", "H2", "H3",
  "H4", "H5", "H6", "HEADER", "HR", "LI", "MAIN", "NAV", "OL", "P", "PRE", "SECTION", "TABLE", "TR", "UL",
]);

const cls = (el: Element) => (typeof el.className === "string" ? el.className : el.getAttribute("class") ?? "");
export const isExample = (el: Element) => EXAMPLE_RE.test(cls(el));
export const isSense = (el: Element) => SENSE_RE.test(cls(el));
export const isPhonetic = (el: Element) => PHONETIC_RE.test(cls(el));
/** 像音标的文字：含国际音标专用符号，或写在 /…/、[…] 里（牛津把「NAmE」这类标签也放在 phon 开头的 class 里） */
const IPA_RE = /[ˈˌːəɪʊʃʒθðŋæɑɒɔʌɜɛɐɚɝ]|^\s*[/[]/;
const looksIpa = (el: Element) => IPA_RE.test(el.textContent ?? "");
/** 以编号开头的块（牛津的 n-g：<span class="z_n">1</span> 释义…）也是一个义项 */
function isNumbered(el: Element): boolean {
  if (!BLOCK_TAGS.has(el.tagName)) return false;
  const first = el.firstElementChild;
  if (!first || !/^\d{1,2}[.)]?$/.test((first.textContent ?? "").trim())) return false;
  // 编号前面不能有别的文字
  for (let n = el.firstChild; n && n !== first; n = n.nextSibling) if ((n.textContent ?? "").trim()) return false;
  return true;
}
const senseLike = (el: Element) => isSense(el) || isNumbered(el);
function hasAncestor(el: Element, test: (e: Element) => boolean): boolean {
  for (let p = el.parentElement; p; p = p.parentElement) if (test(p)) return true;
  return false;
}

/** 解析成 DOM 片段，去掉样式表、脚本、发音按钮；按选项去掉例句。 */
export function parseEntry(html: string, opts: CopyOptions): HTMLElement {
  const doc = new DOMParser().parseFromString(`<div>${html}</div>`, "text/html");
  const root = doc.body.firstElementChild as HTMLElement;
  root.querySelectorAll("style, link, script, audio, video, [data-sound]").forEach((n) => n.remove());
  if (!opts.includeExamples) root.querySelectorAll("*").forEach((n) => isExample(n) && n.remove());
  return root;
}

const hasSenseAncestor = (el: Element) => hasAncestor(el, senseLike);

/** 括号内侧、词与词之间多余的空格（词典 HTML 的换行缩进留下的） */
function tidy(line: string): string {
  return line
    .replace(/(\S)[ \t]{2,}(?=\S)/g, "$1 ")
    // Turndown 把文字片段开头的「=」当成标题下划线转义；不在行首时不需要
    .replace(/(\S.*?)\\=/g, "$1=")
    .replace(/([(（[]) +/g, "$1")
    .replace(/ +([)）\]])/g, "$1");
}

function service(opts: CopyOptions): TurndownService {
  const td = new TurndownService({ headingStyle: "atx", bulletListMarker: "-", codeBlockStyle: "fenced", emDelimiter: "*" });
  // Turndown 中后添加的规则优先：通用规则先加，义项/例句/音标这些具体规则后加
  // 空的 span、font 等纯样式标签只留内容
  td.addRule("plain", {
    filter: (node) => ["SPAN", "FONT", "SUP", "SUB", "SMALL", "BIG", "U"].includes(node.nodeName),
    replacement: (content) => content,
  });

  td.addRule("phonetic", {
    filter: (node) => node.nodeType === 1 && isPhonetic(node as Element) && looksIpa(node as Element),
    replacement: (content) => {
      const t = content.trim();
      return t ? ` \`${t.replace(/`/g, "")}\` ` : "";
    },
  });
  td.addRule("example", {
    filter: (node) => node.nodeType === 1 && isExample(node as Element),
    replacement: (content, node) => {
      // 例句组里又套着例句（牛津的 x-g 里有 x）：只有最外层成为列表项
      if (hasAncestor(node as Element, isExample)) return content;
      const t = content.replace(/\*/g, "").replace(/\s+/g, " ").trim();
      if (!t) return "";
      // 只有在义项列表项里才缩进（3 格，对齐「1. 」后的正文）；
      // 否则 4 格缩进 + 前面空行会被 CommonMark 当成代码块
      const el = node as Element;
      const nested = !!el.parentElement?.closest("li") || hasSenseAncestor(el);
      return `\n${nested ? "   " : ""}- *${t}*\n`;
    },
  });
  td.addRule("sense", {
    filter: (node) => node.nodeType === 1 && senseLike(node as Element) && !(node as Element).closest("li"),
    replacement: (content, node) => {
      // 义项里套着义项（牛津的 n-g 里有 def-g）：只有最外层编号
      if (hasSenseAncestor(node as Element)) return content;
      // 去掉词典自带的编号（可能加粗：**1**）
      const t = content.trim().replace(/^(\*{1,3})\d+[.)]?\1\s*|^\d+[.)]?\s+/, "");
      return t ? `\n1. ${t}\n` : "";
    },
  });
  td.addRule("image", {
    filter: "img",
    replacement: (_c, node) => {
      const el = node as HTMLImageElement;
      const src = el.getAttribute("src") ?? "";
      const alt = (el.getAttribute("alt") ?? "").trim();
      if (opts.imagesAsLinks && /^(https?:|data:image\/)/i.test(src) && !src.includes(".localhost/")) {
        return `[${alt || tr("图片")}](${src})`;
      }
      return alt;
    },
  });
  td.addRule("links", {
    filter: "a",
    replacement: (content, node) => {
      const el = node as HTMLAnchorElement;
      const href = el.getAttribute("href") ?? "";
      if (el.hasAttribute("data-external") && /^https?:/i.test(href)) return `[${content}](${href})`;
      return content;
    },
  });
  td.remove(["style", "script"]);
  return td;
}

export function htmlToMarkdown(html: string, opts: CopyOptions): string {
  const md = service(opts).turndown(parseEntry(html, opts));
  return md
    .split("\n")
    .map(tidy)
    .join("\n")
    .replace(/[ \t]+\n/g, "\n")
    // 列表项之间不留空行（紧凑列表），也避免空行后的缩进行被当成代码块
    .replace(/\n{2,}(?=(?: {3})?- |1\. )/g, "\n")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}

/** 纯文本：块级元素换行，例句缩进两个空格。 */
export function htmlToText(html: string, opts: CopyOptions): string {
  const lines: string[] = [];
  let line = "";
  // 只有一个编号的行（义项编号后面的释义另起一块）：接到下一行开头
  let num = "";
  const flush = (indent = "") => {
    const t = tidy(line.replace(/\s+/g, " ").trim());
    line = "";
    if (!t) return;
    if (/^\d{1,2}[.)]?$/.test(t)) {
      num = t;
      return;
    }
    lines.push(indent + (num ? `${num} ` : "") + t);
    num = "";
  };
  const walk = (node: Node, indent: string, inExample: boolean) => {
    if (node.nodeType === 3) {
      line += node.textContent ?? "";
      return;
    }
    if (node.nodeType !== 1) return;
    const el = node as Element;
    if (el.tagName === "BR") return flush(indent);
    // 例句组里套着的例句不再另起一行、不再多缩进（英文和译文留在同一行）
    const example = isExample(el) && !inExample;
    const block = BLOCK_TAGS.has(el.tagName) || example || isSense(el);
    const childIndent = example ? `${indent}  ` : indent;
    if (block) flush(indent);
    for (const c of Array.from(el.childNodes)) walk(c, childIndent, inExample || example);
    if (block) flush(childIndent);
  };
  walk(parseEntry(html, opts), "", false);
  flush();
  return lines.join("\n");
}

export function sourceLine(format: CopyFormat, src: CopySource): string {
  const text = tr("来源：{dict} · {word}", { dict: src.dict, word: src.headword });
  if (format === "md") return `> ${text}`;
  if (format === "html") return `<p style="color:#666;font-size:12px">${escapeHtml(text)}</p>`;
  return text;
}

export function escapeHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

function mdEscape(s: string): string {
  return s.replace(/([\\*_`[\]])/g, "\\$1");
}

export interface ContextMeta {
  sentence: string;
  hlStart: number;
  hlEnd: number;
  title?: string | null;
  url?: string | null;
  app?: string | null;
  date: Date;
}

/** 语境句（SPEC 10）：`> …made tracking **ubiquitous** in…` + `> — [标题](url) · Edge · 2026-10-07` */
export function contextMarkdown(c: ContextMeta): string {
  const s = c.sentence;
  const quote = `${mdEscape(s.slice(0, c.hlStart))}**${mdEscape(s.slice(c.hlStart, c.hlEnd))}**${mdEscape(s.slice(c.hlEnd))}`;
  const day = `${c.date.getFullYear()}-${String(c.date.getMonth() + 1).padStart(2, "0")}-${String(c.date.getDate()).padStart(2, "0")}`;
  const title = c.title ? (c.url && /^https?:/i.test(c.url) ? `[${mdEscape(c.title)}](${c.url})` : mdEscape(c.title)) : null;
  const app = c.app?.replace(/\.exe$/i, "") ?? null;
  const tail = [title, app, day].filter(Boolean).join(" · ");
  return `> ${quote}\n> — ${tail}`;
}
