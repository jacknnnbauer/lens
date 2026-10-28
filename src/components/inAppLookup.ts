// Lens 窗口内查词（设置 › 取词 › 窗口内查词）：在释义、例句、语境句里双击 / 单击 / 划选一个词就查它。
// 主窗口里直接跳到查词页查这个词；查词小窗里则在小窗原地换词。所在句子都存为语境句。
// 参考 Obsidian 插件 MDX Vocab Lookup 的三种触发方式。只在标了 data-word-lookup 的阅读区生效，
// 按钮、链接、输入框照常工作。词典正文在 Shadow DOM 里，选区与坐标都要按各自的 shadowRoot 取。

import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef } from "react";
import { useSettings } from "../stores/settings";

export type InAppTrigger = "dblclick" | "click" | "drag" | "off";

const SCOPE = "[data-word-lookup]";
const SKIP = "a, button, input, textarea, select, label, summary, [contenteditable], [data-sound], [role=button]";
const MAX_WORD = 40;
const MAX_PHRASE = 60;

/** 事件路径（穿过 Shadow DOM）里有阅读区、且在到达阅读区之前没碰到可交互元素 */
function inScope(e: Event): boolean {
  for (const t of e.composedPath()) {
    if (!(t instanceof Element)) continue;
    // 阅读视图自己处理点词（右侧栏查词、自动收藏）
    if (t.matches("[data-reader]")) return false;
    if (t.matches(SCOPE)) return true;
    if (t.matches(SKIP)) return false;
  }
  return false;
}

function rootOf(e: Event): Document | ShadowRoot {
  const first = e.composedPath()[0];
  const r = first instanceof Node ? first.getRootNode() : document;
  return r instanceof ShadowRoot ? r : document;
}

function selectionIn(root: Document | ShadowRoot): Selection | null {
  // Chromium 的 open shadowRoot 有自己的 getSelection()
  const s = (root as Document & { getSelection?: () => Selection | null }).getSelection?.();
  return s ?? document.getSelection();
}

const WORD_CHAR = /[A-Za-zÀ-ɏ'’-]/;

/** 单击：按坐标找到文字位置，向两边扩到整个词 */
function wordRangeAt(root: Document | ShadowRoot, x: number, y: number): Range | null {
  const doc = document as Document & {
    caretPositionFromPoint?: (x: number, y: number, opts?: { shadowRoots?: ShadowRoot[] }) => { offsetNode: Node; offset: number } | null;
  };
  const pos = doc.caretPositionFromPoint?.(x, y, root instanceof ShadowRoot ? { shadowRoots: [root] } : undefined);
  if (!pos || pos.offsetNode.nodeType !== Node.TEXT_NODE) return null;
  const text = pos.offsetNode.textContent ?? "";
  let a = pos.offset;
  let b = pos.offset;
  while (a > 0 && WORD_CHAR.test(text[a - 1])) a--;
  while (b < text.length && WORD_CHAR.test(text[b])) b++;
  // 去掉词首尾的撇号、连字符
  while (a < b && /['’-]/.test(text[a])) a++;
  while (b > a && /['’-]/.test(text[b - 1])) b--;
  if (a === b) return null;
  const r = document.createRange();
  r.setStart(pos.offsetNode, a);
  r.setEnd(pos.offsetNode, b);
  // 点在词外（行尾空白处）不算
  const rect = r.getBoundingClientRect();
  if (x < rect.left - 2 || x > rect.right + 2 || y < rect.top - 2 || y > rect.bottom + 2) return null;
  return r;
}

/** 词所在的块级段落文字，以及词在其中的 UTF-16 区间（给 Rust 取语境句） */
export function paragraphOf(range: Range): { paragraph: string; sel16: [number, number] } | null {
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
}

export function acceptable(text: string, phrase: boolean): boolean {
  const t = text.trim();
  if (!t || !/[A-Za-z]/.test(t) || /\n/.test(t)) return false;
  if (phrase) return t.length <= MAX_PHRASE && t.split(/\s+/).length <= 5;
  return t.length <= MAX_WORD && !/\s/.test(t);
}

type OnLookup = ((word: string, contextId: number | null) => void) | null;

async function fire(range: Range, text: string, onLookup: OnLookup) {
  const para = paragraphOf(range);
  const word = text.trim();
  if (onLookup) {
    // 主窗口：先存语境句，再在主窗口查（收藏时带上句子）
    const contextId = para
      ? await invoke<number | null>("save_in_app_context", { text: word, paragraph: para.paragraph, sel16: para.sel16 }).catch(() => null)
      : null;
    onLookup(word, contextId);
    return;
  }
  // 查词小窗：原地换词
  await invoke("lookup_in_popup", { text: word, paragraph: para?.paragraph ?? null, sel16: para?.sel16 ?? null, anchor: null }).catch(() => undefined);
}

/** 主窗口传查词回调；查词小窗传 null */
export function useInAppLookup(onLookup: OnLookup) {
  const trigger = (useSettings((s) => s.settings?.capture?.inAppTrigger) ?? "dblclick") as InAppTrigger;

  const onLookupRef = useRef(onLookup);
  onLookupRef.current = onLookup;
  const inPopup = onLookup === null;

  useEffect(() => {
    if (trigger === "off") return;
    const fromSelection = (e: Event, phrase: boolean) => {
      // 事件派发完后 composedPath() 就空了：shadowRoot 要在这里先取出来
      const root = rootOf(e);
      // 等浏览器把双击 / 拖选的选区定下来
      setTimeout(() => {
        const sel = selectionIn(root);
        if (!sel || sel.isCollapsed || sel.rangeCount === 0) return;
        const text = sel.toString();
        if (!acceptable(text, phrase)) return;
        void fire(sel.getRangeAt(0), text, onLookupRef.current);
      }, 0);
    };
    const onDbl = (e: MouseEvent) => {
      if (e.button !== 0 || !inScope(e)) return;
      fromSelection(e, false);
    };
    const onUp = (e: MouseEvent) => {
      if (e.button !== 0 || e.detail > 1 || !inScope(e)) return;
      fromSelection(e, true);
    };
    const onClick = (e: MouseEvent) => {
      if (e.button !== 0 || e.detail > 1 || e.ctrlKey || e.altKey || e.shiftKey || e.metaKey || !inScope(e)) return;
      // 刚拖选完的松手也会触发 click：有选区时不当作单击
      const sel = selectionIn(rootOf(e));
      if (sel && !sel.isCollapsed) return;
      const range = wordRangeAt(rootOf(e), e.clientX, e.clientY);
      if (!range) return;
      const text = range.toString();
      if (acceptable(text, false)) void fire(range, text, onLookupRef.current);
    };
    const [type, fn] = trigger === "dblclick" ? (["dblclick", onDbl] as const) : trigger === "drag" ? (["mouseup", onUp] as const) : (["click", onClick] as const);
    document.addEventListener(type, fn);
    return () => document.removeEventListener(type, fn);
  }, [trigger, inPopup]);
}
