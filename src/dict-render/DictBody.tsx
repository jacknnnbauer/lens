// 在 Shadow DOM 里渲染已清洗的词典正文：词典 CSS 只作用于自己的卡片，不污染界面，也不被界面样式影响。
// 链接点击在宿主侧统一处理（词典里的脚本不会执行）。

import { openUrl } from "@tauri-apps/plugin-opener";
import { useLayoutEffect, useRef } from "react";
import { useAppliedTheme } from "../theme/useApplyTheme";
import { adjustContrast, parseColor, type RGB } from "./contrast";
import { lensUrl } from "./lensUrl";
import { scopeCss } from "./scopeCss";
import { tr } from "../i18n";

// 主题变量穿透 Shadow 边界继承；词典自带 CSS 写在后面，可以覆盖这里的默认值
// :host 用 flow-root：有的词典把整个词条包在 float:left 的容器里（如美国传统词典的 #results），
// 普通 block 会塌成 0 高，卡片底边距失效、内容压到边框上
export const BASE_CSS = `
:host { display: flow-root; color: var(--text); font-family: var(--font-gloss); font-size: var(--gloss-size, 17px);
  line-height: 1.65; overflow-wrap: anywhere; }
.lens-plain { white-space: pre-wrap; }
img { max-width: 100%; height: auto; }
table { max-width: 100%; }
a { color: var(--accent); }
a[data-sound], a[data-entry] { cursor: pointer; }
`;

interface Props {
  html: string;
  onEntry: (word: string) => void;
  /** 标在宿主元素上（data-dict-body），复制 HTML 时据此找到已渲染的正文 */
  dictId?: string;
}

/** 词典样式表最多等这么久，超时也照常显示 */
const STYLESHEET_WAIT_MS = 1500;

function waitForLink(l: HTMLLinkElement): Promise<void> {
  return new Promise<void>((resolve) => {
    if (l.sheet) return resolve();
    l.addEventListener("load", () => resolve(), { once: true });
    l.addEventListener("error", () => resolve(), { once: true });
    setTimeout(resolve, STYLESHEET_WAIT_MS);
  });
}

/** 词典样式表原文，按地址缓存（同一部词典的每张卡片都引用同一份） */
const cssText = new Map<string, Promise<string | null>>();

function fetchCss(href: string): Promise<string | null> {
  let p = cssText.get(href);
  if (!p) {
    p = Promise.race([
      fetch(href).then((r) => (r.ok ? r.text() : null)),
      new Promise<null>((resolve) => setTimeout(() => resolve(null), STYLESHEET_WAIT_MS)),
    ]).catch(() => null);
    cssText.set(href, p);
    // 失败的不缓存，下次再试
    void p.then((t) => t === null && cssText.delete(href));
  }
  return p;
}

/** 词典里的 @font-face 注册到页面外层（Shadow DOM 里声明的不生效），同一条只注册一次 */
const registeredFonts = new Set<string>();

function registerFonts(fonts: string[]) {
  const fresh = fonts.filter((f) => !registeredFonts.has(f));
  if (fresh.length === 0) return;
  for (const f of fresh) registeredFonts.add(f);
  let sheet = document.getElementById("lens-dict-fonts");
  if (!sheet) {
    sheet = document.createElement("style");
    sheet.id = "lens-dict-fonts";
    document.head.append(sheet);
  }
  sheet.textContent += `${fresh.join("\n")}\n`;
}

/**
 * 把词典样式改写成能在 Shadow DOM 里生效的样子（见 scopeCss）：
 * <link> 取回原文改写后换成 <style>；词条里内联的 <style> 就地改写。取不到原文的 <link> 原样留着。
 */
function applyDictStyles(root: ShadowRoot, dictId: string | undefined): Promise<void> {
  const dictBase = dictId ? lensUrl(dictId, "") : location.href;
  const styles = Array.from(root.querySelectorAll("style")).slice(1);
  for (const st of styles) {
    const { css, fonts } = scopeCss(st.textContent ?? "", dictBase, dictBase);
    st.textContent = css;
    registerFonts(fonts);
  }
  const links = Array.from(root.querySelectorAll("link"));
  return Promise.all(
    links.map(async (l) => {
      const href = l.href;
      const text = href ? await fetchCss(href) : null;
      if (text === null || !l.isConnected) return waitForLink(l);
      const { css, fonts } = scopeCss(text, href, dictBase);
      registerFonts(fonts);
      const st = document.createElement("style");
      st.textContent = css;
      l.replaceWith(st);
    }),
  ).then(() => undefined);
}

function cssVarColor(el: Element, name: string): RGB | null {
  return parseColor(getComputedStyle(el).getPropertyValue(name))?.rgb ?? null;
}

function play(url: string) {
  new Audio(url).play().catch((e) => console.warn(tr("发音播放失败"), url, e));
}

export function DictBody({ html, onEntry, dictId }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const onEntryRef = useRef(onEntry);
  onEntryRef.current = onEntry;
  const theme = useAppliedTheme((s) => s.theme);
  const dark = useAppliedTheme((s) => s.dark);

  useLayoutEffect(() => {
    const el = host.current;
    if (!el) return;
    const root = el.shadowRoot ?? el.attachShadow({ mode: "open" });
    root.innerHTML = `<style>${BASE_CSS}</style>${html}`;

    // 词典 CSS 加载完后修正对比度；修正前先隐藏，避免闪一下看不清的原始配色
    let cancelled = false;
    el.style.visibility = "hidden";
    void applyDictStyles(root, dictId).then(() => {
      if (cancelled) return;
      const panel = cssVarColor(el, "--panel");
      const raised = cssVarColor(el, "--raised");
      if (panel && raised) {
        adjustContrast(root, { panel, raised, dark });
      }
      el.style.visibility = "";
    });

    // 词典 mdd 里缺的图片（发音小喇叭等）不显示破图标
    const hideBroken = (img: HTMLImageElement) => {
      img.style.display = "none";
    };
    root.querySelectorAll("img").forEach((img) => {
      if (img.complete && img.naturalWidth === 0 && img.getAttribute("src")) hideBroken(img);
      else img.addEventListener("error", () => hideBroken(img), { once: true });
    });

    const onClick = (ev: Event) => {
      const a = (ev.target as Element | null)?.closest?.("a");
      if (!a) return;
      const href = a.getAttribute("href") ?? "";
      if (a.hasAttribute("data-sound")) {
        ev.preventDefault();
        play(a.getAttribute("data-sound") ?? "");
      } else if (a.hasAttribute("data-entry")) {
        ev.preventDefault();
        const word = a.getAttribute("data-entry") ?? "";
        if (word) onEntryRef.current(word);
      } else if (href.startsWith("#")) {
        ev.preventDefault();
        const id = decodeURIComponent(href.slice(1));
        const target = id ? (root.getElementById(id) ?? root.querySelector(`[name="${CSS.escape(id)}"]`)) : null;
        target?.scrollIntoView({ block: "start", behavior: "smooth" });
      } else if (a.hasAttribute("data-external")) {
        ev.preventDefault();
        void openUrl(href);
      } else {
        // 其他链接（lens:// 资源等）不在窗口内导航
        ev.preventDefault();
      }
    };
    root.addEventListener("click", onClick);
    return () => {
      cancelled = true;
      root.removeEventListener("click", onClick);
    };
  }, [html, theme, dark, dictId]);

  return <div ref={host} data-dict-body={dictId} data-word-lookup="" />;
}
