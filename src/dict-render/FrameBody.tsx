// 带脚本的词典词条：放进沙盒 iframe（sandbox="allow-scripts"，不同源）渲染，词典自己的 JS 照常执行，
// 但碰不到 Lens 的界面、Tauri 接口和本地文件。资源仍走 lens:// 协议（外部 CSS / JS 优先，再找 mdd）。
// iframe 里的小脚本（frameRuntime）报高度、转发链接点击和取词，这里接住再按普通卡片的方式处理。

import { openUrl } from "@tauri-apps/plugin-opener";
import { useEffect, useMemo, useRef, useState } from "react";
import { useSettings } from "../stores/settings";
import { useAppliedTheme } from "../theme/useApplyTheme";
import { contrastKit, parseColor, type RGB } from "./contrast";
import { frameRuntime, type FrameMessage, type FrameOpts } from "./frameRuntime";
import { lensBase } from "./lensUrl";

/** 释义区用到的主题变量，抄进 iframe（CSS 变量穿不过 iframe） */
const VARS = ["--text", "--accent", "--font-gloss", "--gloss-size", "--panel", "--raised", "--muted", "--border"];

/** 窗口内查词：iframe 里取到的词交给 useInAppLookup（见 inAppLookup.ts） */
export const FRAME_LOOKUP_EVENT = "lens-frame-lookup";
export interface FrameLookupDetail {
  text: string;
  phrase: boolean;
  paragraph: string | null;
  sel16: [number, number] | null;
}

function color(el: Element, name: string): RGB | null {
  return parseColor(getComputedStyle(el).getPropertyValue(name))?.rgb ?? null;
}

function scrollParent(el: HTMLElement): HTMLElement | null {
  for (let p = el.parentElement; p; p = p.parentElement) {
    const o = getComputedStyle(p).overflowY;
    if ((o === "auto" || o === "scroll") && p.scrollHeight > p.clientHeight) return p;
  }
  return null;
}

/**
 * 沙盒 iframe 刚建好时，WebView2 不把里面的 lens:// 请求交给 Lens（直接失败，之后才正常）。
 * 所以把词条里会自动加载 lens 资源的地方都先改成 data-lens-*，脚本改成不执行的类型，
 * 由 frameRuntime 探测到协议可用后按原顺序恢复。
 */
export function deferResources(html: string, base: string): string {
  const doc = new DOMParser().parseFromString(`<body>${html}</body>`, "text/html");
  const body = doc.body;
  // 解析器会把开头的 <link>/<style>/<script> 挪进 <head>，挪回来保持原顺序
  body.prepend(...Array.from(doc.head.childNodes));
  const isLens = (u: string | null) => !!u && u.startsWith(base);
  body.querySelectorAll("script").forEach((s) => {
    const type = s.getAttribute("type");
    // 不是 JS 的（模板、JSON 数据）原样留着
    if (type && !/^(text|application)\/(java|ecma)script$|^module$/i.test(type.trim())) return;
    if (type) s.setAttribute("data-lens-type", type);
    s.setAttribute("type", "text/x-lens-defer");
    const src = s.getAttribute("src");
    if (src !== null) {
      s.setAttribute("data-lens-src", src);
      s.removeAttribute("src");
    }
  });
  body.querySelectorAll("link[href]").forEach((l) => {
    if (!isLens(l.getAttribute("href"))) return;
    l.setAttribute("data-lens-href", l.getAttribute("href") ?? "");
    l.removeAttribute("href");
  });
  body.querySelectorAll("style").forEach((st) => {
    if (!(st.textContent ?? "").includes(base)) return;
    st.setAttribute("data-lens-media", st.getAttribute("media") ?? "");
    st.setAttribute("media", "not all");
  });
  body.querySelectorAll("[style]").forEach((e) => {
    const v = e.getAttribute("style") ?? "";
    if (!v.includes(base)) return;
    e.setAttribute("data-lens-style", v);
    e.removeAttribute("style");
  });
  for (const attr of ["src", "poster"]) {
    body.querySelectorAll(`[${attr}]:not(script)`).forEach((e) => {
      if (!isLens(e.getAttribute(attr))) return;
      e.setAttribute(`data-lens-${attr}`, e.getAttribute(attr) ?? "");
      e.removeAttribute(attr);
    });
  }
  return body.innerHTML;
}

export function buildFrameDoc(html: string, baseCss: string, vars: string, opts: FrameOpts): string {
  const base = baseCss.replace(":host", "html, body");
  html = deferResources(html, opts.base);
  const runtime = `(${frameRuntime.toString()})(${contrastKit.toString()}, ${JSON.stringify(opts)});`;
  return (
    `<!doctype html><html><head><meta charset="utf-8">` +
    `<style id="lens-hide">html{visibility:hidden}</style>` +
    `<style>:root{${vars}color-scheme:${opts.dark ? "dark" : "light"}}html,body{margin:0;padding:0;background:transparent}${base}</style>` +
    `<script>${runtime.replace(/<\/script/gi, "<\\/script")}</script>` +
    `</head><body>${html}</body></html>`
  );
}

interface Props {
  html: string;
  baseCss: string;
  onEntry: (word: string) => void;
  dictId?: string;
}

export function FrameBody({ html, baseCss, onEntry, dictId }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const frame = useRef<HTMLIFrameElement>(null);
  const [height, setHeight] = useState(0);
  const onEntryRef = useRef(onEntry);
  onEntryRef.current = onEntry;
  const theme = useAppliedTheme((s) => s.theme);
  const dark = useAppliedTheme((s) => s.dark);
  const trigger = (useSettings((s) => s.settings?.capture?.inAppTrigger) ?? "dblclick") as FrameOpts["trigger"];
  const [doc, setDoc] = useState("");

  // 主题变量要等宿主挂上去才读得到
  useEffect(() => {
    const el = host.current;
    if (!el) return;
    const cs = getComputedStyle(el);
    const vars = VARS.map((v) => `${v}:${cs.getPropertyValue(v).trim()};`).join("");
    // 阅读视图自己处理点词，不在 iframe 里取词
    const inReader = !!el.closest("[data-reader]");
    const opts: FrameOpts = {
      trigger: inReader ? "off" : trigger,
      panel: color(el, "--panel"),
      raised: color(el, "--raised"),
      dark,
      base: lensBase(),
    };
    setDoc(buildFrameDoc(html, baseCss, vars, opts));
  }, [html, baseCss, theme, dark, trigger]);

  useEffect(() => {
    const onMessage = (ev: MessageEvent) => {
      if (!frame.current || ev.source !== frame.current.contentWindow) return;
      const m = ev.data as FrameMessage;
      if (!m || m.lensFrame !== true) return;
      if (m.type === "height") setHeight(m.height);
      else if (m.type === "entry") onEntryRef.current(m.word);
      else if (m.type === "external" && /^(https?|mailto):/i.test(m.url)) void openUrl(m.url);
      else if (m.type === "scroll") {
        const f = frame.current;
        const sp = scrollParent(f);
        if (sp) sp.scrollTo({ top: sp.scrollTop + f.getBoundingClientRect().top - sp.getBoundingClientRect().top + m.top, behavior: "smooth" });
      } else if (m.type === "lookup") {
        const detail: FrameLookupDetail = { text: m.text, phrase: m.phrase, paragraph: m.paragraph, sel16: m.sel16 };
        window.dispatchEvent(new CustomEvent(FRAME_LOOKUP_EVENT, { detail }));
      }
    };
    window.addEventListener("message", onMessage);
    return () => window.removeEventListener("message", onMessage);
  }, []);

  const style = useMemo(() => ({ width: "100%", border: 0, display: "block", height: `${height}px` }), [height]);

  return (
    <div ref={host} data-dict-body={dictId} data-dict-frame="">
      {doc && <iframe ref={frame} title={dictId ?? "dict"} sandbox="allow-scripts" srcDoc={doc} style={style} />}
    </div>
  );
}
