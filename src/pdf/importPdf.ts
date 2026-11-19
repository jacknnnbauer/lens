// 导入一个 PDF：读字节 → pdf.js 逐页取文字（有进度、可取消）→ 判断扫描版 → 重排 → 交给 Rust 写成书。
// pdf.js 和 Tauri 调用都通过 deps 传进来，测试时换成假的。

import { invoke } from "@tauri-apps/api/core";
import { tr } from "../i18n";
import { loadPdf, PdfPasswordError, readMeta, readOutline, readPage } from "./pdfjs";
import { hyphenCandidates, isScanned, reflow, type PdfOutline, type PdfPage, type ReflowBook } from "./reflow";

/** 扫描版：没有可选的文字 */
export class PdfScannedError extends Error {
  constructor() {
    super("scanned");
    this.name = "PdfScannedError";
  }
}

export interface PdfDoc {
  numPages: number;
  destroy(): unknown;
}

export interface PdfSaveBook {
  title: string;
  author: string | null;
  chapters: ReflowBook["chapters"];
  toc: ReflowBook["toc"];
}

export interface PdfDeps {
  read(path: string): Promise<Uint8Array>;
  load(data: Uint8Array): Promise<PdfDoc>;
  page(doc: PdfDoc, n: number): Promise<PdfPage>;
  outline(doc: PdfDoc): Promise<PdfOutline[]>;
  meta(doc: PdfDoc): Promise<{ title: string | null; author: string | null }>;
  wordsExist(words: string[]): Promise<boolean[]>;
  save(path: string, book: PdfSaveBook): Promise<number>;
}

type Real = Parameters<typeof readPage>[0];

export const tauriDeps: PdfDeps = {
  read: (path) => invoke<ArrayBuffer>("pdf_read", { path }).then((b) => new Uint8Array(b)),
  load: (data) => loadPdf(data),
  page: (doc, n) => readPage(doc as unknown as Real, n),
  outline: (doc) => readOutline(doc as unknown as Real),
  meta: (doc) => readMeta(doc as unknown as Real),
  wordsExist: (words) => (words.length ? invoke<boolean[]>("words_exist", { words }) : Promise.resolve([])),
  save: (path, book) => invoke<number>("pdf_import", { path, book }),
};

/** 先看这么多页：都是扫描的就不用往下读了 */
const EARLY_CHECK = 12;

const aborted = () => new DOMException(tr("已取消"), "AbortError");

/** 导入，返回书的 id。`onProgress(已读页数, 总页数)` */
export async function importPdf(path: string, onProgress: (done: number, total: number) => void, signal: AbortSignal, deps: PdfDeps = tauriDeps): Promise<number> {
  const data = await deps.read(path);
  if (signal.aborted) throw aborted();
  const doc = await deps.load(data);
  try {
    const total = doc.numPages;
    const pages: PdfPage[] = [];
    for (let n = 1; n <= total; n++) {
      if (signal.aborted) throw aborted();
      pages.push(await deps.page(doc, n));
      onProgress(n, total);
      if (n === Math.min(EARLY_CHECK, total) && isScanned(pages)) throw new PdfScannedError();
    }
    if (signal.aborted) throw aborted();
    if (isScanned(pages)) throw new PdfScannedError();
    const [outline, meta] = await Promise.all([deps.outline(doc), deps.meta(doc)]);
    const candidates = hyphenCandidates(pages);
    const exists = await deps.wordsExist(candidates);
    const words = new Set(candidates.filter((_, i) => exists[i]));
    const book = reflow(pages, outline, (w) => words.has(w));
    if (book.chapters.length === 0) throw new PdfScannedError();
    if (signal.aborted) throw aborted();
    const stem = path.split(/[\\/]/).pop()?.replace(/\.pdf$/i, "") ?? "PDF";
    return await deps.save(path, { title: meta.title ?? stem, author: meta.author, chapters: book.chapters, toc: book.toc });
  } finally {
    doc.destroy();
  }
}

/** 给用户看的失败原因 */
export function pdfErrorMessage(e: unknown): string {
  if (e instanceof PdfScannedError) return tr("这是扫描版 PDF，没有可选的文字，暂时打不开。");
  if (e instanceof PdfPasswordError) return tr("这个 PDF 设了打开密码，Lens 打不开。");
  // Rust 那边的错误本身就是一句给人看的话
  if (typeof e === "string" && e) return e;
  return tr("这个文件打不开，可能已损坏。");
}
