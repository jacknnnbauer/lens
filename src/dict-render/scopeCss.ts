// 词典 CSS 搬进 Shadow DOM 前的改写（MDict 里的词条是整页渲染，Lens 里是放在卡片的 Shadow DOM 里）：
//
// - Shadow DOM 里没有 <html> / <body>：词典写给 `body { … }`、`html body .x`、`:root` 的样式落空，
//   字体、底色、行距都丢了 → 这些选择器改成 `:host`（`body.dark .x` → `:host(.dark) .x`）；
// - Chromium 不认 Shadow DOM 里声明的 @font-face：音标字体、图标字体显示成方块 → 把 @font-face 拆出来，
//   由调用方注册到页面外层（字体注册在文档上，Shadow DOM 里照样能用这个 font-family）；
// - 样式表从 <link>（url() 相对样式表地址解析）挪进 <style>（相对页面地址解析）→ url() 和 @import 先转成绝对地址；
//   以 `/` 开头的路径在词典里指词典根目录（MDict 的习惯），不是协议根。

export interface ScopedCss {
  /** 放进 Shadow DOM 的样式（不含 @font-face） */
  css: string;
  /** 要注册到页面外层的 @font-face 规则 */
  fonts: string[];
}

/** 嵌套规则（里面还是普通规则，要继续改写） */
const NESTED_AT = /^@(media|supports|layer|document|container)\b/i;

function stripComments(css: string): string {
  return css.replace(/\/\*[\s\S]*?\*\//g, "");
}

/** 从 i（指向 `{`）开始找到配对的 `}`，返回它后面的位置；跳过字符串里的括号 */
function blockEnd(css: string, i: number): number {
  let depth = 0;
  let quote = "";
  for (let k = i; k < css.length; k++) {
    const c = css[k];
    if (quote) {
      if (c === "\\") k++;
      else if (c === quote) quote = "";
    } else if (c === '"' || c === "'") {
      quote = c;
    } else if (c === "{") {
      depth++;
    } else if (c === "}") {
      depth--;
      if (depth === 0) return k + 1;
    }
  }
  return css.length;
}

/** 下一个顶层的 `{` 或 `;`（字符串、括号里的不算） */
function preludeEnd(css: string, i: number): number {
  let quote = "";
  let paren = 0;
  for (let k = i; k < css.length; k++) {
    const c = css[k];
    if (quote) {
      if (c === "\\") k++;
      else if (c === quote) quote = "";
    } else if (c === '"' || c === "'") {
      quote = c;
    } else if (c === "(") {
      paren++;
    } else if (c === ")") {
      paren = Math.max(0, paren - 1);
    } else if (paren === 0 && (c === "{" || c === ";")) {
      return k;
    }
  }
  return css.length;
}

/** 顶层逗号拆选择器列表（`:is(a, b)` 里的逗号不拆） */
function splitSelectors(list: string): string[] {
  const out: string[] = [];
  let depth = 0;
  let cur = "";
  for (const c of list) {
    if (c === "(" || c === "[") depth++;
    else if (c === ")" || c === "]") depth = Math.max(0, depth - 1);
    if (c === "," && depth === 0) {
      out.push(cur);
      cur = "";
    } else {
      cur += c;
    }
  }
  out.push(cur);
  return out.map((s) => s.trim()).filter(Boolean);
}

/** `html` / `:root` / `body`（及其后的 .class / #id / [attr] / :pseudo）→ `:host` / `:host(…)` */
export function scopeSelector(selector: string): string {
  let s = selector.trim();
  let host = false;
  let compound = "";
  const html = /^(?:html|:root)(?![\w-])((?:[.#[][^\s>+~]*)?)\s*(?:>\s*)?/i.exec(s);
  if (html) {
    host = true;
    compound += html[1];
    s = s.slice(html[0].length);
  }
  const body = /^body(?![\w-])((?:[.#[:][^\s>+~]*)?)\s*(?:>\s*)?/i.exec(s);
  if (body) {
    host = true;
    compound += body[1];
    s = s.slice(body[0].length);
  }
  if (!host) return selector.trim();
  const hostSel = compound ? `:host(${compound})` : ":host";
  return s ? `${hostSel} ${s}` : hostSel;
}

/**
 * url(...) 与 @import "..." 里的相对地址转成绝对地址（data: 和已是绝对地址的不动）。
 * rootUrl：词典资源根目录，`/fonts/a.ttf` 按它解析；省略时按 baseUrl 的协议根。
 */
export function absolutizeUrls(css: string, baseUrl: string, rootUrl?: string): string {
  const abs = (raw: string) => {
    const v = raw.trim().replace(/\\/g, "/");
    if (!v || /^(?:data:|[a-z][a-z0-9+.-]*:|#)/i.test(v)) return v;
    try {
      if (rootUrl && v.startsWith("/") && !v.startsWith("//")) return new URL(v.replace(/^\/+/, ""), rootUrl).href;
      return new URL(v, baseUrl).href;
    } catch {
      return v;
    }
  };
  return css
    .replace(/url\(\s*(['"]?)([^'")]*)\1\s*\)/gi, (_w, _q, raw: string) => `url("${abs(raw)}")`)
    .replace(/@import\s+(['"])([^'"]+)\1/gi, (_w, _q, raw: string) => `@import url("${abs(raw)}")`);
}

function scopeRules(css: string, fonts: string[]): string {
  let out = "";
  let i = 0;
  while (i < css.length) {
    const end = preludeEnd(css, i);
    const prelude = css.slice(i, end).trim();
    if (end >= css.length) {
      break;
    }
    if (css[end] === ";") {
      // @import、@charset、@namespace 这类语句
      if (prelude) out += `${prelude};\n`;
      i = end + 1;
      continue;
    }
    const close = blockEnd(css, end);
    const inner = css.slice(end + 1, close - 1);
    if (/^@font-face\b/i.test(prelude)) {
      fonts.push(`@font-face {${inner}}`);
    } else if (NESTED_AT.test(prelude)) {
      out += `${prelude} {\n${scopeRules(inner, fonts)}}\n`;
    } else if (prelude.startsWith("@")) {
      // @keyframes、@page 等：原样保留
      out += `${prelude} {${inner}}\n`;
    } else if (prelude) {
      out += `${splitSelectors(prelude).map(scopeSelector).join(", ")} {${inner}}\n`;
    }
    i = close;
  }
  return out;
}

/**
 * 改写一份词典样式表，供放进 Shadow DOM。
 * baseUrl：样式表自己的地址（来自 <link>），或词典资源根目录（词条里内联的 <style>）；rootUrl：词典资源根目录。
 */
export function scopeCss(css: string, baseUrl: string, rootUrl?: string): ScopedCss {
  const fonts: string[] = [];
  const scoped = scopeRules(stripComments(absolutizeUrls(css, baseUrl, rootUrl)), fonts);
  return { css: scoped, fonts };
}
