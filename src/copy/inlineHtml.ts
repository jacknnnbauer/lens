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

/** 父元素已经是粗体 / 斜体：子元素不再重复包 <b> <i> */
interface Emph {
  bold: boolean;
  italic: boolean;
}

/** Lens 自己的资源（发音图标、词典图片）：离开 Lens 打不开，标记出来，复制前转成内嵌的 data 地址 */
const isLensUrl = (u: string) => /^lens:/i.test(u) || /^https?:\/\/[^/]*\.localhost\//i.test(u);

function imageCopy(el: Element, doc: Document): HTMLImageElement | null {
  const srcUrl = el.getAttribute("src") ?? "";
  const img = doc.createElement("img");
  img.setAttribute("src", srcUrl);
  if (isLensUrl(srcUrl)) img.setAttribute("data-embed", "");
  else if (!/^(https?:|data:image\/)/i.test(srcUrl)) return null;
  const alt = el.getAttribute("alt");
  if (alt) img.setAttribute("alt", alt);
  // 按 Lens 里显示的大小（发音图标原图比显示的大，不写尺寸粘到 Word 里会很大）
  const r = el.getBoundingClientRect();
  if (r.width > 0 && r.height > 0) {
    img.setAttribute("width", String(Math.round(r.width)));
    img.setAttribute("height", String(Math.round(r.height)));
  }
  return img;
}

function cloneInto(src: Node, dest: Node, doc: Document, opts: InlineOptions, parentColor: string, parent: Emph = { bold: false, italic: false }) {
  for (const n of Array.from(src.childNodes)) {
    if (n.nodeType === 3) {
      dest.appendChild(doc.createTextNode(n.textContent ?? ""));
      continue;
    }
    if (n.nodeType !== 1) continue;
    const el = n as HTMLElement;
    if (SKIP.has(el.tagName)) continue;
    if (!opts.includeExamples && isExample(el)) continue;
    const cs = getComputedStyle(el);
    if (cs.display === "none") continue;

    // 发音按钮：只留里面的图标（喇叭图），按钮文字、链接都不要
    if (el.hasAttribute("data-sound")) {
      for (const i of Array.from(el.querySelectorAll("img"))) {
        if (getComputedStyle(i).display === "none") continue;
        const img = imageCopy(i, doc);
        if (img) dest.appendChild(img);
      }
      continue;
    }
    if (el.tagName === "IMG") {
      const img = imageCopy(el, doc);
      if (img) dest.appendChild(img);
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
    dest.appendChild(copy);
    // 除了样式再包一层 <b> <i> <u>：WPS、邮件、网页编辑器、Obsidian 粘贴时常把 style 整个扔掉，只认这些标签
    const bold = isBold(cs);
    const italic = cs.fontStyle === "italic";
    const underline = (cs.textDecorationLine || cs.textDecoration || "").includes("underline");
    let inner: HTMLElement = copy;
    if (bold && !parent.bold && !BOLD_TAGS.has(el.tagName)) inner = inner.appendChild(doc.createElement("b"));
    if (italic && !parent.italic && !ITALIC_TAGS.has(el.tagName)) inner = inner.appendChild(doc.createElement("i"));
    if (underline && !["U", "INS"].includes(el.tagName)) inner = inner.appendChild(doc.createElement("u"));
    cloneInto(el, inner, doc, opts, cs.color, { bold, italic });
  }
}

/** root：DictBody 的 shadowRoot（或任意已渲染的容器）。 */
export function inlineHtml(root: ParentNode, opts: InlineOptions): string {
  const holder = document.createElement("div");
  const host = root instanceof ShadowRoot ? root.host : (root as Element);
  const hs = getComputedStyle(host);
  cloneInto(root, holder, document, opts, hs.color, { bold: isBold(hs), italic: hs.fontStyle === "italic" });
  return holder.innerHTML.trim();
}

/** 一张 Lens 图片 → data 地址；太大（>300 KB）或取不到时 null */
async function loadAsDataUrl(url: string): Promise<string | null> {
  try {
    const r = await fetch(url);
    if (!r.ok) return null;
    const blob = await r.blob();
    if (blob.size > 300_000 || !blob.type.startsWith("image/")) return null;
    return await new Promise<string>((ok, fail) => {
      const fr = new FileReader();
      fr.onload = () => ok(String(fr.result));
      fr.onerror = () => fail(fr.error);
      fr.readAsDataURL(blob);
    });
  } catch {
    return null;
  }
}

/** 把 inlineHtml 标了 data-embed 的图片换成内嵌的 data 地址（粘到哪里都能显示）；取不到的去掉 */
export async function embedImages(html: string, load: (url: string) => Promise<string | null> = loadAsDataUrl): Promise<string> {
  if (!html.includes("data-embed")) return html;
  const holder = document.createElement("div");
  holder.innerHTML = html;
  const imgs = Array.from(holder.querySelectorAll<HTMLImageElement>("img[data-embed]"));
  const cache = new Map<string, Promise<string | null>>();
  await Promise.all(
    imgs.map(async (img) => {
      const u = img.getAttribute("src") ?? "";
      if (!cache.has(u)) cache.set(u, load(u));
      const data = await cache.get(u)!;
      if (data) {
        img.setAttribute("src", data);
        img.removeAttribute("data-embed");
      } else img.remove();
    }),
  );
  return holder.innerHTML;
}

const BOLD_TAGS = new Set(["B", "STRONG", "H1", "H2", "H3", "H4", "H5", "H6", "TH"]);
const ITALIC_TAGS = new Set(["I", "EM", "CITE", "DFN", "VAR"]);
const isBold = (cs: CSSStyleDeclaration) => cs.fontWeight === "bold" || parseInt(cs.fontWeight, 10) >= 600;

/**
 * 已渲染正文里「看得见」的部分：去掉 display:none 的元素（词典常用 CSS 隐藏内部编号等），
 * 保留 class（Markdown / 纯文本转换靠 class 识别义项、例句）。
 * 还要按实际显示保留版式：牛津高阶这类词典全用 span、靠 CSS 分块和加粗，只看标签会挤成一行、丢掉粗斜体——
 * 显示成块的 span 换成 div；词典 CSS 加粗 / 斜体的文字包上 <b> / <i>。
 */
export function visibleHtml(root: ParentNode): string {
  const holder = document.createElement("div");
  const walk = (src: Node, dest: Node, bold: boolean, italic: boolean) => {
    for (const n of Array.from(src.childNodes)) {
      if (n.nodeType === 3) {
        dest.appendChild(document.createTextNode(n.textContent ?? ""));
      } else if (n.nodeType === 1) {
        const el = n as HTMLElement;
        const cs = getComputedStyle(el);
        if (SKIP.has(el.tagName) || cs.display === "none") continue;
        const blockish = /^(block|flex|grid|list-item|table)/.test(cs.display);
        let copy: HTMLElement;
        if (blockish && ["SPAN", "FONT"].includes(el.tagName)) {
          copy = document.createElement("div");
          for (const a of Array.from(el.attributes)) copy.setAttribute(a.name, a.value);
        } else copy = el.cloneNode(false) as HTMLElement;
        copy.removeAttribute("style");
        dest.appendChild(copy);
        const b = isBold(cs);
        const i = cs.fontStyle === "italic";
        let inner: HTMLElement = copy;
        if (b && !bold && !BOLD_TAGS.has(el.tagName)) inner = inner.appendChild(document.createElement("b"));
        if (i && !italic && !ITALIC_TAGS.has(el.tagName)) inner = inner.appendChild(document.createElement("i"));
        walk(el, inner, b, i);
      }
    }
  };
  const host = root instanceof ShadowRoot ? root.host : (root as Element);
  const hs = host instanceof Element ? getComputedStyle(host) : null;
  walk(root, holder, !!hs && isBold(hs), hs?.fontStyle === "italic");
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
