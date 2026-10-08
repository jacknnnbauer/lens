// 在 Shadow DOM 里渲染已清洗的词典正文：词典 CSS 只作用于自己的卡片，不污染界面，也不被界面样式影响。
// 链接点击在宿主侧统一处理（词典里的脚本不会执行）。

import { openUrl } from "@tauri-apps/plugin-opener";
import { useLayoutEffect, useRef } from "react";
import { THEMES } from "../theme/themes";
import { useAppliedTheme } from "../theme/useApplyTheme";
import { adjustContrast, parseColor, type RGB } from "./contrast";

// 主题变量穿透 Shadow 边界继承；词典自带 CSS 写在后面，可以覆盖这里的默认值
export const BASE_CSS = `
:host { display: block; color: var(--text); font-family: var(--font-gloss); font-size: var(--gloss-size, 17px);
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

function waitForStylesheets(root: ShadowRoot): Promise<void> {
  const links = Array.from(root.querySelectorAll("link"));
  return Promise.all(
    links.map(
      (l) =>
        new Promise<void>((resolve) => {
          if (l.sheet) return resolve();
          l.addEventListener("load", () => resolve(), { once: true });
          l.addEventListener("error", () => resolve(), { once: true });
          setTimeout(resolve, STYLESHEET_WAIT_MS);
        }),
    ),
  ).then(() => undefined);
}

function cssVarColor(el: Element, name: string): RGB | null {
  return parseColor(getComputedStyle(el).getPropertyValue(name))?.rgb ?? null;
}

function play(url: string) {
  new Audio(url).play().catch((e) => console.warn("发音播放失败", url, e));
}

export function DictBody({ html, onEntry, dictId }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const onEntryRef = useRef(onEntry);
  onEntryRef.current = onEntry;
  const theme = useAppliedTheme((s) => s.theme);

  useLayoutEffect(() => {
    const el = host.current;
    if (!el) return;
    const root = el.shadowRoot ?? el.attachShadow({ mode: "open" });
    root.innerHTML = `<style>${BASE_CSS}</style>${html}`;

    // 词典 CSS 加载完后修正对比度；修正前先隐藏，避免闪一下看不清的原始配色
    let cancelled = false;
    el.style.visibility = "hidden";
    void waitForStylesheets(root).then(() => {
      if (cancelled) return;
      const panel = cssVarColor(el, "--panel");
      const raised = cssVarColor(el, "--raised");
      if (panel && raised) {
        const dark = THEMES.find((t) => t.id === theme)?.dark ?? false;
        adjustContrast(root, { panel, raised, dark });
      }
      el.style.visibility = "";
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
  }, [html, theme]);

  return <div ref={host} data-dict-body={dictId} />;
}
