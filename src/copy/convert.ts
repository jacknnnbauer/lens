// 复制格式转换（SPEC 10）。输入为 prepare.ts 清洗、改写过的词条 HTML。
//
// 词典 HTML 千差万别，「义项 / 例句 / 音标」只能按常见 class 名识别，尽力而为：
// - 义项（sense/def/meaning…）→ Markdown 有序列表（统一写 `1.`，渲染器自动编号），去掉自带的数字
// - 例句（example/exa/eg/x…）→ 斜体子列表；includeExamples=false 时整段去掉
// - 音标（phon/pron/ipa…）→ 行内代码
// - 图片：只有 http(s) / data: 地址才保留为链接，lens:// 资源离开 Lens 就打不开，直接去掉
// - 发音按钮、页内锚点、站内查词链接只保留文字

import TurndownService from "turndown";

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

/** 解析成 DOM 片段，去掉样式表、脚本、发音按钮；按选项去掉例句。 */
export function parseEntry(html: string, opts: CopyOptions): HTMLElement {
  const doc = new DOMParser().parseFromString(`<div>${html}</div>`, "text/html");
  const root = doc.body.firstElementChild as HTMLElement;
  root.querySelectorAll("style, link, script, audio, video, [data-sound]").forEach((n) => n.remove());
  if (!opts.includeExamples) root.querySelectorAll("*").forEach((n) => isExample(n) && n.remove());
  return root;
}

function hasSenseAncestor(el: Element): boolean {
  for (let p = el.parentElement; p; p = p.parentElement) if (isSense(p)) return true;
  return false;
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
    filter: (node) => node.nodeType === 1 && isPhonetic(node as Element),
    replacement: (content) => {
      const t = content.trim();
      return t ? ` \`${t.replace(/`/g, "")}\` ` : "";
    },
  });
  td.addRule("example", {
    filter: (node) => node.nodeType === 1 && isExample(node as Element),
    replacement: (content, node) => {
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
    filter: (node) => node.nodeType === 1 && isSense(node as Element) && !(node as Element).closest("li"),
    replacement: (content) => {
      const t = content.trim().replace(/^\d+[.)]?\s+/, "");
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
        return `[${alt || "图片"}](${src})`;
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
  const flush = (indent = "") => {
    const t = line.replace(/\s+/g, " ").trim();
    if (t) lines.push(indent + t);
    line = "";
  };
  const walk = (node: Node, indent: string) => {
    if (node.nodeType === 3) {
      line += node.textContent ?? "";
      return;
    }
    if (node.nodeType !== 1) return;
    const el = node as Element;
    if (el.tagName === "BR") return flush(indent);
    const block = BLOCK_TAGS.has(el.tagName) || isExample(el) || isSense(el);
    const childIndent = isExample(el) ? `${indent}  ` : indent;
    if (block) flush(indent);
    for (const c of Array.from(el.childNodes)) walk(c, childIndent);
    if (block) flush(childIndent);
  };
  walk(parseEntry(html, opts), "");
  flush();
  return lines.join("\n");
}

export function sourceLine(format: CopyFormat, src: CopySource): string {
  const text = `来源：${src.dict} · ${src.headword}`;
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
