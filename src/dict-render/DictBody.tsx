// 在 Shadow DOM 里渲染已清洗的词典正文：词典 CSS 只作用于自己的卡片，不污染界面，也不被界面样式影响。
// 链接点击在宿主侧统一处理（词典里的脚本不会执行）。

import { openUrl } from "@tauri-apps/plugin-opener";
import { useLayoutEffect, useRef } from "react";

// 主题变量穿透 Shadow 边界继承；词典自带 CSS 写在后面，可以覆盖这里的默认值
const BASE_CSS = `
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
}

function play(url: string) {
  new Audio(url).play().catch((e) => console.warn("发音播放失败", url, e));
}

export function DictBody({ html, onEntry }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const onEntryRef = useRef(onEntry);
  onEntryRef.current = onEntry;

  useLayoutEffect(() => {
    const el = host.current;
    if (!el) return;
    const root = el.shadowRoot ?? el.attachShadow({ mode: "open" });
    root.innerHTML = `<style>${BASE_CSS}</style>${html}`;

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
    return () => root.removeEventListener("click", onClick);
  }, [html]);

  return <div ref={host} />;
}
