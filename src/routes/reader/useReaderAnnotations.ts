// 阅读页的批注：每次正文排好后把本章批注画上去；选中文字时给出颜色条，点标注给出小卡片，Ctrl+Z 撤销。

import { useCallback, useEffect, useRef, useState, type RefObject } from "react";
import { tr } from "../../i18n";
import type { Annotation, AnnotationColor } from "../../ipc/annotations";
import { locate, makeAnchor } from "../../annotations/anchor";
import { MARK_CLASS, offsetsOf, paint, plainText, unpaint } from "../../annotations/paint";
import { useAnnotations } from "../../stores/annotations";
import { toast } from "../../stores/toast";

/** 选区上方的颜色条：选中的纯文本起止和条的位置 */
export interface BarState {
  start: number;
  end: number;
  x: number;
  y: number;
}

/** 点了一条标注：哪条、卡片位置、是否直接打开笔记框 */
export interface CardState {
  id: number;
  x: number;
  y: number;
  editing: boolean;
}

interface Options {
  body: RefObject<HTMLDivElement | null>;
  articleId: number | undefined;
  /** 当前章（普通文章 0） */
  chapter: number;
  /** 正文每重新排一次加一 */
  rendered: number;
  /** 正文看得见、能标注（不在编辑、不在 PDF 原版视图） */
  enabled: boolean;
}

const isTyping = () => {
  const a = document.activeElement as HTMLElement | null;
  return !!a && (a.tagName === "INPUT" || a.tagName === "TEXTAREA" || a.isContentEditable);
};

/** 带「撤销」按钮的提示 */
export function toastUndo(text: string) {
  toast(text, null, {
    label: tr("撤销"),
    run: () => void useAnnotations.getState().undo().then((t) => t && toast(t)),
  });
}

export function useReaderAnnotations({ body, articleId, chapter, rendered, enabled }: Options) {
  const items = useAnnotations((s) => s.items);
  const loadedFor = useAnnotations((s) => s.articleId);
  const [missing, setMissing] = useState<Set<number>>(new Set());
  const [bar, setBar] = useState<BarState | null>(null);
  const [card, setCard] = useState<CardState | null>(null);
  const flash = useRef<number | null>(null);

  useEffect(() => {
    if (articleId !== undefined) void useAnnotations.getState().load(articleId);
  }, [articleId]);

  // 正文排好、或批注变了：重画本章
  useEffect(() => {
    const el = body.current;
    if (!el || rendered === 0 || loadedFor !== articleId) return;
    unpaint(el);
    const text = plainText(el);
    const found: { id: number; start: number; end: number; color: string; note: boolean }[] = [];
    const lost = new Set<number>();
    for (const a of items) {
      if (a.chapter !== chapter) continue;
      const at = locate(text, a);
      if (at) found.push({ id: a.id, start: at[0], end: at[1], color: a.color, note: !!a.note });
      else lost.add(a.id);
    }
    paint(el, found);
    setMissing((old) => {
      // 别章的「找不到」保留（只有排到那一章才知道）
      const next = new Set([...old].filter((id) => items.some((a) => a.id === id && a.chapter !== chapter)));
      for (const id of lost) next.add(id);
      return next;
    });
    const f = flash.current;
    if (f !== null) {
      const mark = el.querySelector<HTMLElement>(`mark.${MARK_CLASS}[data-hl="${f}"]`);
      if (mark) {
        flash.current = null;
        mark.scrollIntoView({ block: "center", behavior: "smooth" });
        const all = el.querySelectorAll<HTMLElement>(`mark.${MARK_CLASS}[data-hl="${f}"]`);
        all.forEach((m) => m.classList.add("lens-hl-flash"));
        setTimeout(() => all.forEach((m) => m.classList.remove("lens-hl-flash")), 1200);
      }
    }
  }, [body, items, rendered, chapter, articleId, loadedFor]);

  // 选中文字松手：在选区上方给出颜色条
  const onMouseUp = useCallback(() => {
    if (!enabled) return;
    setTimeout(() => {
      const el = body.current;
      const sel = window.getSelection();
      if (!el || !sel || sel.isCollapsed || sel.rangeCount === 0) return setBar(null);
      const range = sel.getRangeAt(0);
      const at = offsetsOf(el, range);
      if (!at || !sel.toString().trim()) return setBar(null);
      const r = range.getBoundingClientRect();
      setCard(null);
      setBar({ start: at[0], end: at[1], x: r.left + r.width / 2, y: r.top });
    }, 0);
  }, [body, enabled]);

  // 点到已有的标注：打开小卡片（查词照常）
  const onClick = useCallback(
    (e: React.MouseEvent) => {
      if (!enabled) return;
      const sel = window.getSelection();
      if (sel && !sel.isCollapsed) return;
      const mark = (e.target as HTMLElement).closest<HTMLElement>(`mark.${MARK_CLASS}`);
      if (!mark) return setCard(null);
      const r = mark.getBoundingClientRect();
      setBar(null);
      setCard({ id: Number(mark.dataset.hl), x: r.left + r.width / 2, y: r.bottom, editing: false });
    },
    [enabled],
  );

  /** 用颜色条加一条；withNote = 加完直接打开笔记框 */
  const create = useCallback(
    async (color: AnnotationColor, withNote: boolean) => {
      const el = body.current;
      if (!el || !bar || articleId === undefined) return;
      const a = makeAnchor(plainText(el), bar.start, bar.end);
      const row = await useAnnotations.getState().add({ articleId, chapter, ...a, color, note: null, createdAt: null });
      window.getSelection()?.removeAllRanges();
      const pos = bar;
      setBar(null);
      if (withNote) setCard({ id: row.id, x: pos.x, y: pos.y + 24, editing: true });
      else toastUndo(tr("已标注"));
    },
    [body, bar, articleId, chapter],
  );

  /** 从批注列表跳过去：换章的话等排好再滚过去并闪一下 */
  const reveal = useCallback(
    (a: Annotation, goChapter: (idx: number) => void) => {
      flash.current = a.id;
      if (a.chapter !== chapter) goChapter(a.chapter);
      else {
        // 本章：直接触发一次重画来滚动和闪烁
        useAnnotations.setState((s) => ({ items: [...s.items] }));
      }
    },
    [chapter],
  );

  // Ctrl+Z：撤销上一步批注操作（在输入框里不管）
  useEffect(() => {
    if (!enabled) return;
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey) || e.key.toLowerCase() !== "z" || e.shiftKey || isTyping()) return;
      e.preventDefault();
      void useAnnotations
        .getState()
        .undo()
        .then((t) => t && toast(t));
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [enabled]);

  // 滚动或换章时收起颜色条
  useEffect(() => {
    if (!bar) return;
    const close = () => setBar(null);
    window.addEventListener("scroll", close, true);
    return () => window.removeEventListener("scroll", close, true);
  }, [bar]);

  useEffect(() => {
    setBar(null);
    setCard(null);
  }, [chapter, articleId, enabled]);

  return { items, missing, bar, card, setBar, setCard, onMouseUp, onClick, create, reveal };
}
