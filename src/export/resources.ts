// HTML 导出的资源内联（SPEC 11）：词典 CSS（含其中 url() 引用的字体、图片）与图片都转成 data URI，
// 得到可以单独打开、离开 Lens 也完整的单文件。只处理 lens:// 资源；超过 MAX_BYTES 的跳过。

import { isLensUrl } from "../dict-render/lensUrl";

const MAX_BYTES = 2 * 1024 * 1024;

async function fetchBlob(url: string): Promise<Blob | null> {
  try {
    const r = await fetch(url);
    if (!r.ok) return null;
    const b = await r.blob();
    return b.size <= MAX_BYTES ? b : null;
  } catch {
    return null;
  }
}

function toDataUri(b: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const fr = new FileReader();
    fr.onload = () => resolve(String(fr.result));
    fr.onerror = () => reject(fr.error);
    fr.readAsDataURL(b);
  });
}

/** CSS 里的 url(...)：相对样式表地址解析，lens:// 资源换成 data URI。 */
export async function inlineCssUrls(css: string, baseUrl: string): Promise<string> {
  const re = /url\(\s*(['"]?)([^'")]+)\1\s*\)/g;
  const found = new Map<string, string>();
  for (const m of css.matchAll(re)) {
    const raw = m[2].trim();
    if (raw.startsWith("data:") || found.has(raw)) continue;
    let abs: string;
    try {
      abs = new URL(raw, baseUrl).href;
    } catch {
      continue;
    }
    if (!isLensUrl(abs)) continue;
    const blob = await fetchBlob(abs);
    found.set(raw, blob ? await toDataUri(blob) : "");
  }
  return css.replace(re, (whole, _q, raw: string) => {
    const v = found.get(raw.trim());
    return v === undefined ? whole : v ? `url("${v}")` : "none";
  });
}

/** 把一条词条 HTML 里的 lens:// 样式表与图片全部内联。 */
export async function inlineEntry(html: string): Promise<string> {
  const doc = new DOMParser().parseFromString(`<div>${html}</div>`, "text/html");
  const root = doc.body.firstElementChild as HTMLElement;

  for (const link of Array.from(root.querySelectorAll("link"))) {
    const href = link.getAttribute("href") ?? "";
    if (!isLensUrl(href)) {
      link.remove();
      continue;
    }
    try {
      const r = await fetch(href);
      const css = r.ok ? await r.text() : "";
      const style = doc.createElement("style");
      style.textContent = await inlineCssUrls(css, href);
      link.replaceWith(style);
    } catch {
      link.remove();
    }
  }
  for (const img of Array.from(root.querySelectorAll("img"))) {
    const src = img.getAttribute("src") ?? "";
    if (!isLensUrl(src)) continue;
    const blob = await fetchBlob(src);
    if (blob) img.setAttribute("src", await toDataUri(blob));
    else img.remove();
  }
  return root.innerHTML;
}
