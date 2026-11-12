// 沙盒 iframe 里跑的小脚本：带脚本的词典词条在 iframe 里渲染（见 FrameBody），词典自己的 JS 在那里执行。
//
// 这个函数会被 `frameRuntime.toString()` 原样塞进 iframe，**不能引用函数外面的任何东西**
// （import、模块里的常量、tr() 都不行），用到的对比度修正由参数 kitFn 传进来。
//
// 和外面只通过 postMessage 通信：报高度、点了词典内链 / 外链 / 页内锚点、在释义里取词。

import type { contrastKit, RGB } from "./contrast";

export interface FrameOpts {
  /** 窗口内查词的触发方式（设置 › 取词） */
  trigger: "dblclick" | "click" | "drag" | "off";
  panel: RGB | null;
  raised: RGB | null;
  dark: boolean;
  /** lens 协议根地址（如 http://lens.localhost/），探测协议是否已通 */
  base: string;
}

export type FrameMessage =
  | { lensFrame: true; type: "height"; height: number }
  | { lensFrame: true; type: "entry"; word: string }
  | { lensFrame: true; type: "external"; url: string }
  | { lensFrame: true; type: "scroll"; top: number }
  | { lensFrame: true; type: "lookup"; text: string; phrase: boolean; paragraph: string | null; sel16: [number, number] | null };

export function frameRuntime(kitFn: typeof contrastKit, o: FrameOpts): void {
  const post = (m: Record<string, unknown>) => parent.postMessage({ lensFrame: true, ...m }, "*");

  // ---- 高度：iframe 不滚动，高度跟着内容走 ----
  let last = -1;
  const report = () => {
    const b = document.body;
    if (!b) return;
    const cs = getComputedStyle(b);
    let h = Math.ceil(b.getBoundingClientRect().height + (parseFloat(cs.marginTop) || 0) + (parseFloat(cs.marginBottom) || 0));
    // 词典里浮动、绝对定位的内容可能伸出 body（vocabulary.com 的例句翻页）：
    // 文档比 iframe 高就按文档高度来，不然卡片里会多出一条滚动条
    const de = document.documentElement;
    if (de.scrollHeight > de.clientHeight + 1) h = Math.max(h, de.scrollHeight);
    if (h !== last) {
      last = h;
      post({ type: "height", height: h });
    }
  };

  // ---- 样式加载完：修正对比度，再显示 ----
  let shown = false;
  const show = () => {
    if (shown) return;
    shown = true;
    if (o.panel && o.raised) {
      try {
        kitFn().adjustContrast(document.body, { panel: o.panel, raised: o.raised, dark: o.dark });
      } catch {
        // 修不了就按词典原样显示
      }
    }
    document.getElementById("lens-hide")?.remove();
    report();
  };
  setTimeout(show, 4000);
  let observed = false;
  addEventListener("DOMContentLoaded", () => {
    if (observed) return;
    observed = true;
    new ResizeObserver(report).observe(document.body);
    report();
    void boot();
  });

  // ---- 词典的 CSS / 图片 / 脚本：等 lens 协议在这个 iframe 里能用了再加载 ----
  // WebView2 刚建好的沙盒 iframe 里，最初一小段时间发出的 lens:// 请求不会交给 Lens 处理（直接失败），
  // 所以 FrameBody 把这些资源都改成了 data-lens-*，这里先探测协议通了，再按原顺序恢复。
  async function ready(): Promise<void> {
    const until = Date.now() + 3000;
    while (Date.now() < until) {
      // 能拿到响应（哪怕 404）就说明请求到了 Lens：别的服务器不会带 Access-Control-Allow-Origin
      const ok = await fetch(`${o.base}res/~probe`, { cache: "no-store" }).then(
        () => true,
        () => false,
      );
      if (ok) return;
      await new Promise((r) => setTimeout(r, 30));
    }
  }

  function loaded(el: HTMLElement): Promise<void> {
    return new Promise((r) => {
      el.addEventListener("load", () => r(), { once: true });
      el.addEventListener("error", () => r(), { once: true });
      setTimeout(r, 3000);
    });
  }

  async function boot(): Promise<void> {
    await ready();
    const sheets: Promise<void>[] = [];
    document.querySelectorAll<HTMLLinkElement>("link[data-lens-href]").forEach((l) => {
      sheets.push(loaded(l));
      l.href = l.getAttribute("data-lens-href") ?? "";
      l.removeAttribute("data-lens-href");
    });
    document.querySelectorAll<HTMLStyleElement>("style[data-lens-media]").forEach((s) => {
      s.media = s.getAttribute("data-lens-media") ?? "";
      s.removeAttribute("data-lens-media");
    });
    document.querySelectorAll<HTMLElement>("[data-lens-style]").forEach((e) => {
      e.setAttribute("style", e.getAttribute("data-lens-style") ?? "");
      e.removeAttribute("data-lens-style");
    });
    for (const attr of ["src", "poster"]) {
      document.querySelectorAll<HTMLElement>(`[data-lens-${attr}]:not(script)`).forEach((e) => {
        e.setAttribute(attr, e.getAttribute(`data-lens-${attr}`) ?? "");
        e.removeAttribute(`data-lens-${attr}`);
      });
    }
    document.querySelectorAll("audio, video").forEach((m) => (m as HTMLMediaElement).load());
    await Promise.all(sheets);
    // 脚本按原顺序一个个执行（外部脚本等它加载完再执行下一个）
    for (const old of Array.from(document.querySelectorAll<HTMLScriptElement>('script[type="text/x-lens-defer"]'))) {
      const s = document.createElement("script");
      for (const a of Array.from(old.attributes)) {
        if (a.name === "type" || a.name.startsWith("data-lens-")) continue;
        s.setAttribute(a.name, a.value);
      }
      const type = old.getAttribute("data-lens-type");
      if (type) s.type = type;
      const src = old.getAttribute("data-lens-src");
      if (src) {
        s.src = src;
        const done = loaded(s);
        old.replaceWith(s);
        await done;
      } else {
        s.text = old.text;
        old.replaceWith(s);
      }
    }
    // 词典脚本常等 DOMContentLoaded / load 才动手：真的那两次已经过去了，补发一次
    document.dispatchEvent(new Event("DOMContentLoaded", { bubbles: true }));
    dispatchEvent(new Event("load"));
    // 等脚本改完版面（jQuery 的 ready 回调是异步的）
    await new Promise((r) => setTimeout(r, 50));
    show();
  }

  // 词典里缺的图片（发音小喇叭等）不显示破图标
  document.addEventListener(
    "error",
    (e) => {
      const t = e.target as HTMLElement | null;
      if (t && t.tagName === "IMG") t.style.display = "none";
    },
    true,
  );

  // ---- 链接：词典自己的点击处理先跑（冒泡到 document 最后才到这里） ----
  document.addEventListener("click", (e) => {
    const a = (e.target as Element | null)?.closest?.("a");
    if (!a) return;
    const href = a.getAttribute("href") ?? "";
    if (a.hasAttribute("data-sound")) {
      e.preventDefault();
      new Audio(a.getAttribute("data-sound") ?? "").play().catch(() => undefined);
    } else if (a.hasAttribute("data-entry")) {
      e.preventDefault();
      const word = a.getAttribute("data-entry") ?? "";
      if (word) post({ type: "entry", word });
    } else if (a.hasAttribute("data-external")) {
      e.preventDefault();
      post({ type: "external", url: href });
    } else if (href.startsWith("#")) {
      e.preventDefault();
      const id = decodeURIComponent(href.slice(1));
      if (!id) return;
      const t = document.getElementById(id) ?? document.querySelector(`[name="${CSS.escape(id)}"]`);
      if (t) post({ type: "scroll", top: t.getBoundingClientRect().top + scrollY });
    } else if (!/^\s*javascript:/i.test(href)) {
      // 其他地址不在 iframe 里跳转
      e.preventDefault();
    }
  });

  // ---- 窗口内查词：在 iframe 里取词和所在段落，交给外面查 ----
  if (o.trigger === "off") return;
  const SKIP = "a, button, input, textarea, select, label, summary, [contenteditable], [data-sound], [role=button], [onclick]";
  const WORD_CHAR = /[A-Za-zÀ-ɏ'’-]/;

  const paragraphOf = (range: Range): { paragraph: string; sel16: [number, number] } | null => {
    let el: Element | null = range.startContainer instanceof Element ? range.startContainer : range.startContainer.parentElement;
    while (el && el.parentElement && /^inline/.test(getComputedStyle(el).display)) el = el.parentElement;
    if (!el) return null;
    const before = document.createRange();
    before.setStart(el, 0);
    before.setEnd(range.startContainer, range.startOffset);
    const start = before.toString().length;
    const paragraph = el.textContent ?? "";
    const end = start + range.toString().length;
    if (end > paragraph.length || paragraph.length > 2000) return null;
    return { paragraph, sel16: [start, end] };
  };

  const send = (range: Range, text: string, phrase: boolean) => {
    const p = paragraphOf(range);
    post({ type: "lookup", text, phrase, paragraph: p ? p.paragraph : null, sel16: p ? p.sel16 : null });
  };

  const skipped = (e: Event) => !!(e.target as Element | null)?.closest?.(SKIP);

  const fromSelection = (phrase: boolean) =>
    setTimeout(() => {
      const sel = getSelection();
      if (!sel || sel.isCollapsed || sel.rangeCount === 0) return;
      send(sel.getRangeAt(0), sel.toString(), phrase);
    }, 0);

  if (o.trigger === "dblclick") {
    document.addEventListener("dblclick", (e) => {
      if (e.button === 0 && !skipped(e)) fromSelection(false);
    });
  } else if (o.trigger === "drag") {
    document.addEventListener("mouseup", (e) => {
      if (e.button === 0 && e.detail <= 1 && !skipped(e)) fromSelection(true);
    });
  } else {
    document.addEventListener("click", (e) => {
      if (e.button !== 0 || e.detail > 1 || e.ctrlKey || e.altKey || e.shiftKey || e.metaKey || skipped(e)) return;
      const sel = getSelection();
      if (sel && !sel.isCollapsed) return;
      const d = document as Document & { caretPositionFromPoint?: (x: number, y: number) => { offsetNode: Node; offset: number } | null };
      const pos = d.caretPositionFromPoint?.(e.clientX, e.clientY);
      if (!pos || pos.offsetNode.nodeType !== Node.TEXT_NODE) return;
      const text = pos.offsetNode.textContent ?? "";
      let a = pos.offset;
      let b = pos.offset;
      while (a > 0 && WORD_CHAR.test(text[a - 1])) a--;
      while (b < text.length && WORD_CHAR.test(text[b])) b++;
      while (a < b && /['’-]/.test(text[a])) a++;
      while (b > a && /['’-]/.test(text[b - 1])) b--;
      if (a === b) return;
      const r = document.createRange();
      r.setStart(pos.offsetNode, a);
      r.setEnd(pos.offsetNode, b);
      const rect = r.getBoundingClientRect();
      if (e.clientX < rect.left - 2 || e.clientX > rect.right + 2 || e.clientY < rect.top - 2 || e.clientY > rect.bottom + 2) return;
      send(r, r.toString(), false);
    });
  }
}
