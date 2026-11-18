// 词典正文预处理（SPEC 9.3）：在惰性文档里改写链接与资源 → DOMPurify 清洗 → 生成折叠摘要。
//
// - `sound://x` → `data-sound`（lens:// 地址，点击播放）
// - `entry://word`、`bword://word` → `data-entry`（点击在应用内查词）；`entry://#a` → 页内锚点
// - 相对路径 / 以 `/` 开头的路径 → lens://res/<dictId>/…
// - 资源类属性（img/link/audio…）只允许 lens:// 与 data:image，外网资源一律去掉（不发起网络请求）
// - 外链 http(s)/mailto 保留并标 `data-external`，点击时交给系统浏览器

import DOMPurify from "dompurify";
import type { EntryBody } from "../ipc/types";
import { isLensUrl, lensUrl } from "./lensUrl";

export interface PreparedEntry {
  html: string;
  summary: string;
  /** 词条带脚本时：保留脚本的版本，放进沙盒 iframe 里渲染（见 FrameBody） */
  frame?: string;
}

const SUMMARY_CHARS = 120;
const RESOURCE_TAGS = new Set(["img", "link", "audio", "video", "source", "track", "image", "input"]);

function escapeHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

function decodeTarget(s: string): string {
  try {
    return decodeURIComponent(s);
  } catch {
    return s;
  }
}

/** 改写一个元素上的 URL 属性（必要时删除该属性或改成 data-* 属性）。 */
function rewriteUrl(el: Element, attr: string, dictId: string, scripts: boolean): void {
  const raw = el.getAttribute(attr);
  if (raw === null) return;
  const url = raw.trim();
  const tag = el.tagName.toLowerCase();
  const isResource = RESOURCE_TAGS.has(tag) || attr !== "href";
  const scheme = /^([a-z][a-z0-9+.-]*):/i.exec(url)?.[1]?.toLowerCase();

  if (scheme === "sound") {
    el.removeAttribute(attr);
    el.setAttribute("data-sound", lensUrl(dictId, url.replace(/^sound:\/\//i, "")));
    if (tag === "a") el.setAttribute("href", "#");
    return;
  }
  if (scheme === "entry" || scheme === "bword") {
    const target = url.replace(/^[a-z]+:\/\//i, "");
    if (target.startsWith("#")) {
      el.setAttribute(attr, target);
    } else {
      const word = decodeTarget(target.split("#")[0]).trim();
      el.setAttribute("data-entry", word);
      el.setAttribute(attr, "#");
    }
    return;
  }
  if (scheme === "http" || scheme === "https" || scheme === "mailto") {
    if (isLensUrl(url)) return;
    if (isResource) {
      el.removeAttribute(attr);
      el.setAttribute("data-blocked", url);
    } else {
      el.setAttribute("data-external", "");
    }
    return;
  }
  if (scheme === "data") {
    if (!/^data:image\//i.test(url)) el.removeAttribute(attr);
    return;
  }
  if (scheme === "javascript" && scripts) return;
  if (scheme) {
    // javascript:、file: 等其他协议
    el.removeAttribute(attr);
    return;
  }
  if (url.startsWith("#") || url === "") return;
  const path = url.split(/[?#]/)[0];
  el.setAttribute(attr, lensUrl(dictId, path));
}

function rewrite(root: ParentNode, dictId: string, scripts = false): void {
  for (const el of Array.from(root.querySelectorAll("*"))) {
    for (const attr of ["src", "href", "xlink:href", "poster"]) {
      if (el.hasAttribute(attr)) rewriteUrl(el, attr, dictId, scripts);
    }
    if (el.tagName.toLowerCase() === "link") {
      const rel = (el.getAttribute("rel") ?? "").toLowerCase();
      if (!rel.split(/\s+/).includes("stylesheet") || !el.getAttribute("href")) el.remove();
    }
  }
}

/**
 * 词典专用的清洗器（单独实例，钩子不影响阅读器那边的 DOMPurify）。
 * 很多词典用自造标签排版（现汉7 的 <entry> <hw> <def> <num> <ex>），CSS 也按这些标签写；
 * DOMPurify 默认把不认识的标签剥掉只留文字，词典样式就全落空了。这类标签浏览器只当普通的未知元素，
 * 不执行任何东西，所以放行；禁止名单（script、iframe……）在钩子之后检查，照样拦。
 *
 * 第一次清洗时才建：模块加载时就碰 window 的话，只是顺带引到这个文件的测试（不在浏览器环境里跑）会直接报错。
 */
let purifierInstance: ReturnType<typeof DOMPurify> | null = null;
const UNKNOWN_TAG = /^[a-z][a-z0-9]*(?:-[a-z0-9]+)*$/;
function purifier() {
  if (purifierInstance) return purifierInstance;
  const p = DOMPurify(window);
  p.addHook("uponSanitizeElement", (node, data) => {
    const tag = data.tagName;
    if (data.allowedTags[tag] || !UNKNOWN_TAG.test(tag)) return;
    // 只放行 HTML 里的未知元素（SVG / MathML 里的标签按 DOMPurify 原规则处理）
    if (node instanceof HTMLUnknownElement || (tag.includes("-") && node instanceof HTMLElement)) data.allowedTags[tag] = true;
  });
  purifierInstance = p;
  return p;
}

const SANITIZE: Parameters<typeof DOMPurify.sanitize>[1] = {
  FORCE_BODY: true,
  ADD_TAGS: ["link", "style"],
  ADD_ATTR: ["rel", "target", "data-entry", "data-sound", "data-external", "data-blocked"],
  FORBID_TAGS: ["script", "form", "iframe", "object", "embed", "base", "meta", "input", "textarea", "select", "button"],
  ALLOWED_URI_REGEXP: /^(?:https?:|mailto:|#|data:image\/)/i,
};

/** 词条里有脚本：<script>、onclick 之类的事件属性、javascript: 链接 */
const HAS_SCRIPT = /<script\b|\son[a-z]+\s*=|href\s*=\s*["']?\s*javascript:/i;

function summaryOf(html: string): string {
  const doc = new DOMParser().parseFromString(html, "text/html");
  doc.querySelectorAll("style, link").forEach((n) => n.remove());
  const text = (doc.body.textContent ?? "").replace(/\s+/g, " ").trim();
  return text.length > SUMMARY_CHARS ? `${text.slice(0, SUMMARY_CHARS)}…` : text;
}

export function prepareEntry(body: EntryBody, dictId: string): PreparedEntry {
  if (body.type === "text") {
    const text = body.content.replace(/\s+/g, " ").trim();
    return {
      html: `<div class="lens-plain">${escapeHtml(body.content)}</div>`,
      summary: text.length > SUMMARY_CHARS ? `${text.slice(0, SUMMARY_CHARS)}…` : text,
    };
  }
  // DOMParser 生成的文档是惰性的：不执行脚本、不加载资源
  const doc = new DOMParser().parseFromString(body.content, "text/html");
  const holder = doc.createElement("div");
  // 词条开头的 <link>/<style>/<script> 会被解析器挪进 <head>，一并取回（脚本只留在 iframe 版里）
  for (const n of Array.from(doc.head.children)) {
    if (["link", "style", "script"].includes(n.tagName.toLowerCase())) holder.append(n);
  }
  holder.append(...Array.from(doc.body.childNodes));
  // 带脚本的词条另留一份不清洗的（脚本、事件属性、javascript: 链接都保留），只在沙盒 iframe 里用
  let frame: string | undefined;
  if (HAS_SCRIPT.test(body.content)) {
    const copy = holder.cloneNode(true) as HTMLElement;
    rewrite(copy, dictId, true);
    frame = copy.innerHTML;
  }
  rewrite(holder, dictId);
  const clean = purifier().sanitize(holder.innerHTML, { ...SANITIZE, RETURN_DOM_FRAGMENT: true });
  // DOMPurify 会剥掉 <link> 的 rel；rewrite 已只留下样式表，这里只对指向 lens:// 的补回 rel
  for (const link of Array.from(clean.querySelectorAll("link"))) {
    const href = link.getAttribute("href") ?? "";
    if (isLensUrl(href)) link.setAttribute("rel", "stylesheet");
    else link.remove();
  }
  const out = document.createElement("div");
  out.append(clean);
  const html = out.innerHTML;
  return frame === undefined ? { html, summary: summaryOf(html) } : { html, summary: summaryOf(html), frame };
}
