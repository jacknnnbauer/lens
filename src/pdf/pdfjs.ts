// pdf.js 的薄封装：按需加载（不拖慢启动），取每页文字块、书签、书名作者。
// worker 打进前端，不联网。

import type { PDFDocumentProxy } from "pdfjs-dist";
import type { PdfItem, PdfOutline, PdfPage } from "./reflow";

/** 设了打开密码 */
export class PdfPasswordError extends Error {
  constructor() {
    super("password");
    this.name = "PdfPasswordError";
  }
}

let lib: Promise<typeof import("pdfjs-dist")> | null = null;

/** 第一次用时才加载 pdf.js */
export function pdfjs(): Promise<typeof import("pdfjs-dist")> {
  lib ??= Promise.all([import("pdfjs-dist"), import("pdfjs-dist/build/pdf.worker.min.mjs?url")]).then(([m, worker]) => {
    m.GlobalWorkerOptions.workerSrc = worker.default;
    return m;
  });
  return lib;
}

/** 打开 PDF；用完调 destroy() 释放 worker 里的内存 */
export async function loadPdf(data: Uint8Array): Promise<PDFDocumentProxy & { destroy: () => Promise<void> }> {
  const m = await pdfjs();
  const task = m.getDocument({ data, disableFontFace: true, useSystemFonts: false });
  try {
    const doc = await task.promise;
    return Object.assign(doc, { destroy: () => task.destroy() });
  } catch (e) {
    if ((e as { name?: string })?.name === "PasswordException") throw new PdfPasswordError();
    throw e;
  }
}

/** 一页的文字块，坐标换成「从页顶往下」 */
export async function readPage(doc: PDFDocumentProxy, n: number): Promise<PdfPage> {
  const page = await doc.getPage(n);
  try {
    const vp = page.getViewport({ scale: 1 });
    const content = await page.getTextContent();
    const items: PdfItem[] = [];
    for (const it of content.items) {
      if (!("str" in it) || !it.str) continue;
      const t = it.transform as number[];
      const size = Math.hypot(t[2], t[3]) || it.height || 10;
      const [x, baseline] = vp.convertToViewportPoint(t[4], t[5]);
      items.push({ str: it.str, x, y: baseline - size, w: it.width, h: size, size });
    }
    return { page: n, width: vp.width, height: vp.height, items };
  } finally {
    page.cleanup();
  }
}

/** 书签（目录），层级从 0 起；指不到页的跳过 */
export async function readOutline(doc: PDFDocumentProxy): Promise<PdfOutline[]> {
  const out: PdfOutline[] = [];
  const walk = async (items: Awaited<ReturnType<PDFDocumentProxy["getOutline"]>>, level: number) => {
    for (const it of items ?? []) {
      try {
        const dest = typeof it.dest === "string" ? await doc.getDestination(it.dest) : it.dest;
        const ref = Array.isArray(dest) ? dest[0] : null;
        const idx = ref == null ? -1 : typeof ref === "number" ? ref : await doc.getPageIndex(ref);
        if (idx >= 0 && it.title?.trim()) out.push({ title: it.title.trim(), page: idx + 1, level });
      } catch {
        // 坏书签不影响别的
      }
      if (it.items?.length && level < 6) await walk(it.items, level + 1);
    }
  };
  await walk(await doc.getOutline(), 0);
  return out;
}

export async function readMeta(doc: PDFDocumentProxy): Promise<{ title: string | null; author: string | null }> {
  try {
    const { info } = (await doc.getMetadata()) as { info: { Title?: string; Author?: string } };
    const clean = (s?: string) => (s && s.trim() ? s.trim() : null);
    return { title: clean(info?.Title), author: clean(info?.Author) };
  } catch {
    return { title: null, author: null };
  }
}
