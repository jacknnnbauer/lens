// PDF 书的「原版页面」（只电脑版）：pdf.js 一页页照原样显示，竖向连续滚动；
// 带文字层，单击单词、拖选短语都交给阅读页的查词（onPick），和重排视图里一样。

import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import "pdfjs-dist/web/pdf_viewer.css";
import { tr } from "../../i18n";
import { acceptable } from "../../components/inAppLookup";
import { loadPdf, pdfjs } from "../../pdf/pdfjs";
import { pageFor } from "../../pdf/viewMap";
import styles from "./PdfPages.module.css";

/** 打开时停在哪：直接给页码，或者给重排视图里的章节和位置 */
export type PdfStart = { page: number } | { chapter: number; pos: number };

interface Props {
  articleId: number;
  pdfPages: number[];
  start: PdfStart;
  /** 翻到第几页（共几页） */
  onPage: (page: number, total: number) => void;
  onPick: (word: string, range: Range) => void;
}

type Viewer = { currentScale: number; currentScaleValue: string; currentPageNumber: number; pagesCount: number; setDocument(d: unknown): void };

const isWordChar = (c: string | undefined) => !!c && /[\p{L}\p{M}'’-]/u.test(c);

/** 点到的位置所在的整个单词 */
function wordAt(x: number, y: number): { word: string; range: Range } | null {
  const r = document.caretRangeFromPoint?.(x, y);
  const node = r?.startContainer;
  if (!r || !node || node.nodeType !== Node.TEXT_NODE) return null;
  const text = node.textContent ?? "";
  let a = r.startOffset;
  let b = r.startOffset;
  while (a > 0 && isWordChar(text[a - 1])) a--;
  while (b < text.length && isWordChar(text[b])) b++;
  while (a < b && /['’-]/.test(text[a])) a++;
  while (b > a && /['’-]/.test(text[b - 1])) b--;
  const word = text.slice(a, b);
  if (!/\p{L}/u.test(word)) return null;
  const range = document.createRange();
  range.setStart(node, a);
  range.setEnd(node, b);
  return { word, range };
}

export function PdfPages({ articleId, pdfPages, start, onPage, onPick }: Props) {
  const box = useRef<HTMLDivElement>(null);
  const inner = useRef<HTMLDivElement>(null);
  const viewer = useRef<Viewer | null>(null);
  const fit = useRef(true);
  const [page, setPage] = useState({ n: 0, total: 0 });
  const [error, setError] = useState<string | null>(null);
  // 回调和起点只在打开时用一次；之后的变化不重建文档
  const startRef = useRef(start);
  const onPageRef = useRef(onPage);
  onPageRef.current = onPage;

  useEffect(() => {
    let alive = true;
    let destroy: (() => void) | null = null;
    (async () => {
      const lib = await pdfjs();
      // pdf.js 的页面组件从全局取库
      (globalThis as { pdfjsLib?: unknown }).pdfjsLib = lib;
      const v = await import("pdfjs-dist/web/pdf_viewer.mjs");
      const bytes = new Uint8Array(await invoke<ArrayBuffer>("book_pdf", { id: articleId }));
      const doc = await loadPdf(bytes);
      if (!alive || !box.current || !inner.current) {
        void doc.destroy();
        return;
      }
      const eventBus = new v.EventBus();
      const linkService = new v.PDFLinkService({ eventBus });
      const pv = new v.PDFViewer({ container: box.current, viewer: inner.current, eventBus, linkService, textLayerMode: 1 });
      linkService.setViewer(pv);
      pv.setDocument(doc);
      linkService.setDocument(doc);
      viewer.current = pv as unknown as Viewer;
      eventBus.on("pagesinit", () => {
        pv.currentScaleValue = "page-width";
        const s = startRef.current;
        const target = "page" in s ? s.page : pageFor(pdfPages, s.chapter, s.pos, doc.numPages);
        pv.currentPageNumber = Math.min(Math.max(1, target), doc.numPages);
        setPage({ n: pv.currentPageNumber, total: doc.numPages });
      });
      eventBus.on("pagechanging", (e: { pageNumber: number }) => {
        setPage({ n: e.pageNumber, total: doc.numPages });
        onPageRef.current(e.pageNumber, doc.numPages);
      });
      destroy = () => {
        pv.setDocument(null as never);
        void doc.destroy();
      };
    })().catch((e) => alive && setError(String(e)));
    return () => {
      alive = false;
      destroy?.();
      viewer.current = null;
    };
  }, [articleId, pdfPages]);

  // 窗口变宽变窄：适应宽度时跟着缩放
  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      if (fit.current && viewer.current) viewer.current.currentScaleValue = "page-width";
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const zoom = (factor: number | null) => {
    const pv = viewer.current;
    if (!pv) return;
    if (factor === null) {
      fit.current = true;
      pv.currentScaleValue = "page-width";
      return;
    }
    fit.current = false;
    pv.currentScale = Math.min(4, Math.max(0.4, pv.currentScale * factor));
  };

  const onWheel = (e: React.WheelEvent) => {
    if (!e.ctrlKey) return;
    e.preventDefault();
    zoom(e.deltaY < 0 ? 1.1 : 1 / 1.1);
  };

  const onClick = (e: React.MouseEvent) => {
    if (!(e.target as HTMLElement).closest(".textLayer")) return;
    const sel = window.getSelection();
    if (sel && !sel.isCollapsed) return;
    const hit = wordAt(e.clientX, e.clientY);
    if (hit) onPick(hit.word, hit.range);
  };

  const onMouseUp = (e: React.MouseEvent) => {
    if (e.button !== 0 || e.detail > 1) return;
    setTimeout(() => {
      const sel = window.getSelection();
      if (!sel || sel.isCollapsed || sel.rangeCount === 0) return;
      const text = sel.toString().trim();
      if (acceptable(text, true)) onPick(text.replace(/\s+/g, " "), sel.getRangeAt(0));
    }, 0);
  };

  return (
    <div className={styles.wrap}>
      <div className={styles.bar}>
        <span className={styles.count}>{page.total ? `${page.n} / ${page.total}` : tr("正在打开 PDF…")}</span>
        <button type="button" onClick={() => zoom(1 / 1.15)} title={tr("缩小")} aria-label={tr("缩小")}>
          −
        </button>
        <button type="button" onClick={() => zoom(1.15)} title={tr("放大")} aria-label={tr("放大")}>
          +
        </button>
        <button type="button" onClick={() => zoom(null)}>
          {tr("适应宽度")}
        </button>
      </div>
      {error && <p className={styles.error}>{error}</p>}
      <div className={styles.stage}>
        <div ref={box} className={styles.box} onWheel={onWheel} onClick={onClick} onMouseUp={onMouseUp} data-pdf-pages="">
          <div ref={inner} className="pdfViewer" />
        </div>
      </div>
    </div>
  );
}
