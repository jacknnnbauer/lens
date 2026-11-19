// PDF 导入队列：一本一本整理（pdf.js 吃内存，不并行），进度显示在主窗口上，可取消。
// 放在主窗口这一层，不管当前在哪一页都能跑完。

import { create } from "zustand";
import { tr } from "../i18n";
import { importPdf, pdfErrorMessage } from "../pdf/importPdf";
import { useReader } from "./reader";
import { useRoute } from "./route";
import { toast } from "./toast";

interface PdfImportState {
  /** 正在整理的那本：文件名、读到第几页、共几页 */
  running: { name: string; done: number; total: number } | null;
  /** 还在排队的路径 */
  queue: string[];
  /** 每导入成功一本加一：文章库据此刷新 */
  version: number;
  enqueue: (paths: string[]) => void;
  /** 取消正在整理的这本，排队的也不做了 */
  cancel: () => void;
}

let ctrl: AbortController | null = null;

const fileName = (p: string) => p.split(/[\\/]/).pop() ?? p;

export const usePdfImport = create<PdfImportState>((set, get) => {
  async function run() {
    const done: number[] = [];
    while (get().queue.length > 0) {
      const [path, ...rest] = get().queue;
      set({ queue: rest, running: { name: fileName(path), done: 0, total: 0 } });
      ctrl = new AbortController();
      try {
        const id = await importPdf(path, (n, total) => set({ running: { name: fileName(path), done: n, total } }), ctrl.signal);
        done.push(id);
        set((s) => ({ version: s.version + 1 }));
      } catch (e) {
        if ((e as { name?: string })?.name === "AbortError") {
          set({ queue: [] });
          break;
        }
        toast(tr("没有导入 {file}：{why}", { file: fileName(path), why: pdfErrorMessage(e) }));
      }
    }
    ctrl = null;
    set({ running: null });
    if (done.length === 1) {
      useReader.getState().open(done[0]);
      useRoute.getState().go("reader");
    } else if (done.length > 1) {
      toast(tr("已导入 {n} 篇文章", { n: done.length }));
    }
  }

  return {
    running: null,
    queue: [],
    version: 0,
    enqueue: (paths) => {
      const fresh = paths.filter((p) => !get().queue.includes(p));
      if (fresh.length === 0) return;
      const idle = get().running === null;
      set((s) => ({ queue: [...s.queue, ...fresh] }));
      if (idle) void run();
    },
    cancel: () => ctrl?.abort(),
  };
});
