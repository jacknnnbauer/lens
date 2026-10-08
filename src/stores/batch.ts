// 批量导出状态：对话框开关、当前批次（进度、条目）、上次没做完的批次。

import { create } from "zustand";
import {
  batchItems,
  batchStatus,
  cancelBatch,
  discardBatch,
  listUnfinishedBatches,
  onBatchProgress,
  pauseBatch,
  resumeBatch,
  retryFailedBatch,
  type BatchCounts,
  type BatchItem,
  type BatchProgress,
  type BatchSource,
  type BatchState,
  type BatchSummary,
} from "../ipc/batch";

export interface Live {
  state: BatchState;
  counts: BatchCounts | null;
  renderQueue: number;
  memMb: number;
  memLimitMb: number;
  recycles: number;
  /** 本轮开始时间（续传时重新计） */
  startedAt: number | null;
  error: string | null;
}

interface BatchStore {
  open: boolean;
  /** 打开对话框时默认的选词方式 */
  initialSource: BatchSource;
  /** 正在看的批次（进度页）；null 表示在选词页 */
  current: BatchSummary | null;
  items: BatchItem[];
  live: Live | null;
  unfinished: BatchSummary[];
  error: string | null;
  openDialog: (source?: BatchSource) => void;
  closeDialog: () => void;
  /** 进入某个批次的进度页（新开始的、续传的） */
  show: (id: string) => Promise<void>;
  /** 回到选词页 */
  backToSelect: () => void;
  refreshUnfinished: () => Promise<void>;
  pause: () => Promise<void>;
  resume: (id?: string) => Promise<void>;
  cancel: () => Promise<void>;
  retryFailed: () => Promise<void>;
  discard: (id: string) => Promise<void>;
}

const liveOf = (s: BatchSummary): Live => ({
  state: s.state,
  counts: s.counts,
  renderQueue: 0,
  memMb: 0,
  memLimitMb: 0,
  recycles: 0,
  startedAt: null,
  error: null,
});

export const useBatch = create<BatchStore>((set, get) => {
  const run = async (f: () => Promise<unknown>) => {
    try {
      set({ error: null });
      await f();
    } catch (e) {
      set({ error: String(e) });
    }
  };

  return {
    open: false,
    initialSource: "history",
    current: null,
    items: [],
    live: null,
    unfinished: [],
    error: null,

    openDialog: (source) => set((s) => ({ open: true, initialSource: source ?? s.initialSource, error: null })),
    closeDialog: () => set({ open: false }),

    show: async (id) => {
      try {
        const [summary, items] = await Promise.all([batchStatus(id), batchItems(id)]);
        set({ open: true, current: summary, items, live: liveOf(summary), error: null });
      } catch (e) {
        set({ error: String(e) });
      }
    },

    backToSelect: () => set({ current: null, items: [], live: null, error: null }),

    refreshUnfinished: async () => {
      try {
        set({ unfinished: await listUnfinishedBatches() });
      } catch (e) {
        console.warn("读取未完成的批量导出失败", e);
      }
    },

    pause: () => run(async () => {
      const id = get().current?.id;
      if (id) await pauseBatch(id);
    }),

    resume: (id) =>
      run(async () => {
        const target = id ?? get().current?.id;
        if (!target) return;
        await resumeBatch(target);
        if (get().current?.id !== target) await get().show(target);
        set((s) => ({ live: s.live ? { ...s.live, state: "running", error: null } : s.live }));
        await get().refreshUnfinished();
      }),

    cancel: () => run(async () => {
      const id = get().current?.id;
      if (id) await cancelBatch(id);
    }),

    retryFailed: () =>
      run(async () => {
        const id = get().current?.id;
        if (!id) return;
        const n = await retryFailedBatch(id);
        if (n > 0) await get().show(id);
      }),

    discard: (id) =>
      run(async () => {
        await discardBatch(id);
        if (get().current?.id === id) get().backToSelect();
        await get().refreshUnfinished();
      }),
  };
});

/** 订阅进度事件：更新当前批次的计数与条目。整个主窗口只调用一次。 */
export function subscribeBatchEvents(): Promise<() => void> {
  return onBatchProgress((e: BatchProgress) => {
    const { current, items, live } = useBatch.getState();
    if (e.state !== "running" && e.state !== "paused") void useBatch.getState().refreshUnfinished();
    if (!current || current.id !== e.batchId) return;
    const next: Live = {
      state: e.state,
      counts: e.counts ?? live?.counts ?? null,
      renderQueue: e.renderQueue ?? live?.renderQueue ?? 0,
      memMb: e.memMb ?? live?.memMb ?? 0,
      memLimitMb: e.memLimitMb ?? live?.memLimitMb ?? 0,
      recycles: e.recycles ?? live?.recycles ?? 0,
      startedAt: e.startedAt ?? live?.startedAt ?? null,
      error: e.error ?? null,
    };
    let nextItems = items;
    if (e.changed?.length) {
      const bySeq = new Map(e.changed.map((i) => [i.seq, i]));
      nextItems = items.map((i) => bySeq.get(i.seq) ?? i);
    }
    useBatch.setState({ live: next, items: nextItems, current: { ...current, state: e.state } });
  });
}
