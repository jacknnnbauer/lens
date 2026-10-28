// 阅读视图的正文：Markdown → 清洗过的 HTML → 把每个英文词包成 <span class="lens-w">，
// 同时拼出和这些 span 一一对应的纯文本（块与块之间用换行隔开），交给 Rust 的生词扫描。
// 代码块、行内代码、链接、图片占位里的文字不切词，也不进纯文本（点链接是打开链接）。

import DOMPurify from "dompurify";
import { marked } from "marked";

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
