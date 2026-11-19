import { describe, expect, it, vi } from "vitest";
import { importPdf, PdfScannedError, type PdfDeps, type PdfDoc } from "./importPdf";
import { PdfPasswordError } from "./pdfjs";
import type { PdfPage } from "./reflow";

const LINE = "It was the kind of morning that made everyone restless and nobody could quite say why it was.";

function fakePage(n: number, text: string): PdfPage {
  return { page: n, width: 600, height: 800, items: [{ str: text, x: 72, y: 100, w: text.length * 5, h: 10, size: 10 }] };
}

function deps(pages: string[], over: Partial<PdfDeps> = {}): PdfDeps & { save: ReturnType<typeof vi.fn> } {
  const save = vi.fn(async () => 42);
  return {
    read: async () => new Uint8Array([1, 2, 3]),
    load: async () => ({ numPages: pages.length, destroy: () => undefined }) as PdfDoc,
    page: async (_doc, n) => fakePage(n, pages[n - 1]),
    outline: async () => [],
    meta: async () => ({ title: null, author: "Ann Writer" }),
    wordsExist: async (ws) => ws.map(() => false),
    ...over,
    save,
  };
}

describe("importPdf", () => {
  it("reflows and saves with chapter pages, title falls back to the file name", async () => {
    const d = deps([LINE, LINE, LINE]);
    const progress: [number, number][] = [];
    const id = await importPdf("C:\\books\\My Book.pdf", (a, b) => progress.push([a, b]), new AbortController().signal, d);
    expect(id).toBe(42);
    expect(progress).toEqual([
      [1, 3],
      [2, 3],
      [3, 3],
    ]);
    const [path, book] = d.save.mock.calls[0];
    expect(path).toBe("C:\\books\\My Book.pdf");
    expect(book.title).toBe("My Book");
    expect(book.author).toBe("Ann Writer");
    expect(book.chapters.map((c: { page: number }) => c.page)).toEqual([1]);
  });

  it("rejects scanned PDFs without saving", async () => {
    const d = deps(["", "x", "", "ab"]);
    await expect(importPdf("a.pdf", () => undefined, new AbortController().signal, d)).rejects.toBeInstanceOf(PdfScannedError);
    expect(d.save).not.toHaveBeenCalled();
  });

  it("passes the password error through", async () => {
    const d = deps([LINE], {
      load: async () => {
        throw new PdfPasswordError();
      },
    });
    await expect(importPdf("a.pdf", () => undefined, new AbortController().signal, d)).rejects.toBeInstanceOf(PdfPasswordError);
  });

  it("stops when cancelled and saves nothing", async () => {
    const ctrl = new AbortController();
    const d = deps([LINE, LINE, LINE, LINE]);
    const p = importPdf("a.pdf", (n) => n === 2 && ctrl.abort(), ctrl.signal, d);
    await expect(p).rejects.toMatchObject({ name: "AbortError" });
    expect(d.save).not.toHaveBeenCalled();
  });
});
