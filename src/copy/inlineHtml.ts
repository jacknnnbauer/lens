// HTML 复制（SPEC 10）：从已渲染的词条（Shadow DOM）读计算样式，只把白名单属性内联到克隆的元素上，
// 这样粘贴到 Word / OneNote 时保留粗斜体、颜色、字号、缩进，又不带词典 CSS 的其余部分。

import { isExample } from "./convert";

const SKIP = new Set(["STYLE", "LINK", "SCRIPT", "AUDIO", "VIDEO", "SOURCE", "BUTTON", "INPUT"]);
/** 保持原标签的 HTML 元素；词典自定义标签（<entry> <h> 等）按显示方式换成 div / span */
const KNOWN = new Set([
  "A", "ABBR", "B", "BLOCKQUOTE", "BR", "CITE", "CODE", "DD", "DFN", "DIV", "DL", "DT", "EM", "FONT", "H1", "H2",
  "H3", "H4", "H5", "H6", "HR", "I", "IMG", "LI", "MARK", "OL", "P", "PRE", "Q", "S", "SMALL", "SPAN", "STRONG",
  "SUB", "SUP", "TABLE", "TBODY", "TD", "TH", "THEAD", "TR", "U", "UL",
]);

export interface InlineOptions {
  /** 深色主题下词典颜色已被提亮，粘到白纸上看不清，此时不内联颜色 */
  colors: boolean;
  includeExamples: boolean;
}

/** parentColor：父元素的颜色，相同就不重复内联（继承来的颜色不写） */
function styleOf(cs: CSSStyleDeclaration, opts: InlineOptions, parentColor: string): string {
  const out: string[] = [];
  const weight = parseInt(cs.fontWeight, 10);
  if (weight >= 600 || cs.fontWeight === "bold") out.push("font-weight:bold");
  if (cs.fontStyle === "italic") out.push("font-style:italic");
  if ((cs.textDecorationLine || cs.textDecoration || "").includes("underline")) out.push("text-decoration:underline");
  if (opts.colors && cs.color && cs.color !== parentColor) out.push(`color:${cs.color}`);
  const size = parseFloat(cs.fontSize);
  if (size && Math.abs(size - 16) > 0.5) out.push(`font-size:${Math.round(size)}px`);
  for (const p of ["margin-left", "padding-left"] as const) {
    const v = parseFloat(cs.getPropertyValue(p));
    if (v > 0.5) out.push(`${p}:${Math.round(v)}px`);
  }
  if (cs.verticalAlign === "super" || cs.verticalAlign === "sub") out.push(`vertical-align:${cs.verticalAlign}`);
  return out.join(";");
}

function cloneInto(src: Node, dest: Node, doc: Document, opts: InlineOptions, parentColor: string) {
  for (const n of Array.from(src.childNodes)) {
    if (n.nodeType === 3) {
      dest.appendChild(doc.createTextNode(n.textContent ?? ""));
      continue;
    }
    if (n.nodeType !== 1) continue;
    const el = n as HTMLElement;
    if (SKIP.has(el.tagName) || el.hasAttribute("data-sound")) continue;
    if (!opts.includeExamples && isExample(el)) continue;
    const cs = getComputedStyle(el);
    if (cs.display === "none") continue;

    if (el.tagName === "IMG") {
      const srcUrl = el.getAttribute("src") ?? "";
      // lens:// 资源离开 Lens 打不开；只保留网络图片与内嵌图片
      if (/^(https?:|data:image\/)/i.test(srcUrl) && !srcUrl.includes(".localhost/")) {
        const img = doc.createElement("img");
        img.setAttribute("src", srcUrl);
        img.setAttribute("alt", el.getAttribute("alt") ?? "");
        dest.appendChild(img);
      }
      continue;
    }

    const blockish = /^(block|flex|grid|list-item|table)/.test(cs.display);
    // 自定义标签按显示方式换；span/font 被词典 CSS 设成块级时（如每条例句一行）也换成 div，
    // 否则粘到 Word 里会挤成一段
    let tag = !KNOWN.has(el.tagName) || (blockish && ["SPAN", "FONT"].includes(el.tagName))
      ? (blockish ? "div" : "span")
      : el.tagName.toLowerCase();
    let href: string | null = null;
    if (tag === "a") {
      const h = el.getAttribute("href") ?? "";
      if (el.hasAttribute("data-external") && /^https?:/i.test(h)) href = h;
      else tag = "span";
    }
    const copy = doc.createElement(tag);
    if (href) copy.setAttribute("href", href);
    // 去掉了链接的 <a>（站内查词、页内锚点）不再带链接色，看起来就是普通文字
    const delinked = el.tagName === "A" && !href;
    const style = styleOf(cs, delinked ? { ...opts, colors: false } : opts, parentColor);
    if (style) copy.setAttribute("style", style);
    cloneInto(el, copy, doc, opts, cs.color);
    dest.appendChild(copy);
  }
}

/** root：DictBody 的 shadowRoot（或任意已渲染的容器）。 */
export function inlineHtml(root: ParentNode, opts: InlineOptions): string {
  const holder = document.createElement("div");
  const host = root instanceof ShadowRoot ? root.host : (root as Element);
  cloneInto(root, holder, document, opts, getComputedStyle(host).color);
  return holder.innerHTML.trim();
}

/**
 * 已渲染正文里「看得见」的部分：去掉 display:none 的元素（词典常用 CSS 隐藏内部编号等），
 * 保留标签与 class（Markdown / 纯文本转换靠 class 识别义项、例句）。
 */
export function visibleHtml(root: ParentNode): string {
  const holder = document.createElement("div");
  const walk = (src: Node, dest: Node) => {
    for (const n of Array.from(src.childNodes)) {
      if (n.nodeType === 3) {
        dest.appendChild(document.createTextNode(n.textContent ?? ""));
      } else if (n.nodeType === 1) {
        const el = n as HTMLElement;
        if (SKIP.has(el.tagName) || getComputedStyle(el).display === "none") continue;
        const copy = el.cloneNode(false) as HTMLElement;
        copy.removeAttribute("style");
        dest.appendChild(copy);
        walk(el, copy);
      }
    }
  };
  walk(root, holder);
  return holder.innerHTML;
}

/** 卡片折叠（没渲染）时的退路：只保留标签结构，不带样式与 class。 */
export function basicHtml(entryHtml: string, opts: InlineOptions): string {
  const doc = new DOMParser().parseFromString(`<div>${entryHtml}</div>`, "text/html");
  const root = doc.body.firstElementChild as HTMLElement;
  root.querySelectorAll("style, link, script, audio, video, [data-sound]").forEach((n) => n.remove());
  if (!opts.includeExamples) root.querySelectorAll("*").forEach((n) => isExample(n) && n.remove());
  root.querySelectorAll("*").forEach((n) => {
    for (const a of Array.from(n.attributes)) {
      if (!(n.tagName === "A" && a.name === "href" && /^https?:/i.test(a.value))) n.removeAttribute(a.name);
    }
    if (n.tagName === "IMG") n.remove();
  });
  return root.innerHTML.trim();
}
