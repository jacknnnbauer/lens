// 阅读视图的正文：Markdown → 清洗过的 HTML → 把每个英文词包成 <span class="lens-w">，
// 同时拼出和这些 span 一一对应的纯文本（块与块之间用换行隔开），交给 Rust 的生词扫描。
// 代码块、行内代码、链接、图片占位里的文字不切词，也不进纯文本（点链接是打开链接）。

import DOMPurify from "dompurify";
import { marked } from "marked";
import { bookUrl } from "../../dict-render/lensUrl";

/** 包词的 span 的 class；样式在 ReaderPage.module.css 里按 :global 写 */
export const WORD_CLASS = "lens-w";

const WORD = /[A-Za-z]+(?:['’][A-Za-z]+)*/g;
const BLOCK = new Set(["P", "LI", "H1", "H2", "H3", "H4", "H5", "H6", "BLOCKQUOTE", "TD", "TH", "DT", "DD", "FIGCAPTION", "DIV", "PRE", "TR"]);
const SKIP = "pre, code, a, kbd, samp, script, style, .lens-img";

export function renderMarkdown(md: string, imageLabel: (alt: string) => string): string {
  const html = marked.parse(md, { async: false, gfm: true }) as string;
  const clean = DOMPurify.sanitize(html, { FORBID_TAGS: ["style", "form", "input", "button", "iframe", "object", "embed"], FORBID_ATTR: ["style"] });
  // 图片不加载（文章里的远程图片会联网，本地相对路径也找不到）：换成写着图片说明的占位
  const tpl = document.createElement("template");
  tpl.innerHTML = clean;
  for (const img of Array.from(tpl.content.querySelectorAll("img"))) {
    const span = document.createElement("span");
    span.className = "lens-img";
    span.textContent = imageLabel(img.getAttribute("alt") ?? "");
    img.replaceWith(span);
  }
  return tpl.innerHTML;
}

/** 书里的相对路径解析成压缩包里的路径和锚点；`base` 是当前章节的路径，和 Rust 的 epub::resolve 同一规则 */
export function resolveBookPath(base: string, href: string): [string, string | null] {
  const hash = href.indexOf("#");
  const decode = (s: string) => {
    try {
      return decodeURIComponent(s);
    } catch {
      return s;
    }
  };
  const path = decode(hash < 0 ? href : href.slice(0, hash));
  const anchor = hash < 0 ? null : decode(href.slice(hash + 1)) || null;
  if (!path) return [base, anchor];
  const parts = path.startsWith("/") ? [] : base.split("/").slice(0, -1);
  for (const seg of path.split("/")) {
    if (seg === "..") parts.pop();
    else if (seg && seg !== ".") parts.push(seg);
  }
  return [parts.join("/"), anchor];
}

/** 章节的 body，放进 HTML 文档：先按 XHTML 解析（`<a id="x"/>` 才是空的），写得不规范再按 HTML */
function chapterBody(raw: string): HTMLElement {
  const parser = new DOMParser();
  const xml = parser.parseFromString(raw, "application/xhtml+xml");
  const body = xml.querySelector("parsererror") ? null : xml.querySelector("body");
  const source = body ?? parser.parseFromString(raw, "text/html").body;
  return document.implementation.createHTMLDocument("").importNode(source, true) as HTMLElement;
}

/** 一章的 XHTML → 清洗过的 HTML。样式用阅读页自己的，图片从书里取 */
export function renderBookChapter(raw: string, base: string, articleId: number, imageLabel: (alt: string) => string): string {
  const body = chapterBody(raw);
  // 封面常写成 <svg><image xlink:href="…">：换成普通图片
  for (const svg of Array.from(body.querySelectorAll("svg"))) {
    const image = svg.querySelector("image");
    const href = image?.getAttribute("href") ?? image?.getAttribute("xlink:href");
    if (!href) continue;
    const img = body.ownerDocument.createElement("img");
    img.setAttribute("src", href);
    svg.replaceWith(img);
  }
  const clean = DOMPurify.sanitize(body.innerHTML, {
    FORBID_TAGS: ["style", "link", "form", "input", "button", "iframe", "object", "embed", "svg", "math", "audio", "video"],
    FORBID_ATTR: ["style", "class", "width", "height", "align", "bgcolor", "border"],
  });
  const tpl = document.createElement("template");
  tpl.innerHTML = clean;
  for (const img of Array.from(tpl.content.querySelectorAll("img"))) {
    const src = img.getAttribute("src") ?? "";
    if (/^data:/i.test(src)) continue;
    // 书里的图片才加载；网络图片会联网，换成占位
    if (!src || /^[a-z][a-z0-9+.-]*:/i.test(src)) {
      const span = document.createElement("span");
      span.className = "lens-img";
      span.textContent = imageLabel(img.getAttribute("alt") ?? "");
      img.replaceWith(span);
      continue;
    }
    img.setAttribute("src", bookUrl(articleId, resolveBookPath(base, src)[0]));
    img.setAttribute("loading", "lazy");
  }
  return tpl.innerHTML;
}

function blockOf(node: Node, root: HTMLElement): Node {
  let el: Node | null = node.parentNode;
  while (el && el !== root) {
    if (el instanceof HTMLElement && BLOCK.has(el.tagName)) return el;
    el = el.parentNode;
  }
  return root;
}

export interface WordIndex {
  /** 和 span 对应的纯文本 */
  plain: string;
  /** 纯文本里的起点（UTF-16）→ span */
  byStart: Map<number, HTMLElement>;
}

/** 在已经放好 HTML 的容器里包词，返回纯文本和位置索引 */
export function wrapWords(root: HTMLElement): WordIndex {
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    acceptNode: (n) => (n.parentElement?.closest(SKIP) ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT),
  });
  const nodes: Text[] = [];
  for (let n = walker.nextNode(); n; n = walker.nextNode()) nodes.push(n as Text);

  let plain = "";
  let lastBlock: Node | null = null;
  const byStart = new Map<number, HTMLElement>();
  for (const node of nodes) {
    const block = blockOf(node, root);
    if (lastBlock && block !== lastBlock && plain && !plain.endsWith("\n")) plain += "\n";
    lastBlock = block;
    const text = node.data;
    const base = plain.length;
    plain += text;
    WORD.lastIndex = 0;
    const matches = [...text.matchAll(WORD)];
    if (matches.length === 0) continue;
    const frag = document.createDocumentFragment();
    let pos = 0;
    for (const m of matches) {
      const i = m.index ?? 0;
      if (i > pos) frag.append(text.slice(pos, i));
      const span = document.createElement("span");
      span.className = WORD_CLASS;
      span.textContent = m[0];
      span.dataset.start = String(base + i);
      byStart.set(base + i, span);
      frag.append(span);
      pos = i + m[0].length;
    }
    if (pos < text.length) frag.append(text.slice(pos));
    node.replaceWith(frag);
  }
  return { plain, byStart };
}
