// 英文怎么说：当前这条（输入 + 对象描述）的状态。结果由 Rust 缓存，这里只管显示；
// 请求还没回来时换了输入，旧结果回来就丢掉。
// 「更多说法」词条只跟输入有关；换了输入或对象就收起。

import { create } from "zustand";
import { aiKeyStatus } from "../ipc/ai";
import {
  expressionCached,
  expressionEntryCached,
  expressionEntryGenerate,
  expressionGenerate,
  type EntryResult,
  type ExpressionResult,
} from "../ipc/expression";

export type ExpressionStatus = "idle" | "loading" | "done" | "error" | "nokey";

interface ExpressionState {
  key: string | null;
  status: ExpressionStatus;
  result?: ExpressionResult;
  error?: string;
  /** 「更多说法」是否展开 */
  entryOpen: boolean;
  entryStatus: ExpressionStatus;
  entryResult?: EntryResult;
  entryError?: string;
  /** 输入或对象变了：有缓存就直接显示（不花钱），没有就回到折叠态 */
  show: (input: string, audience: string | null) => Promise<void>;
  /** 点开 / 换一批：生成（命令先查缓存，force 时重新生成） */
  run: (input: string, audience: string | null, force?: boolean) => Promise<void>;
  /** 展开「更多说法」：先读缓存（不花钱），没有再生成；force = 换一批 */
  openEntry: (input: string, force?: boolean) => Promise<void>;
  closeEntry: () => void;
  reset: () => void;
}

const keyOf = (input: string, audience: string | null) => `${input}\u0000${audience ?? ""}`;
const noEntry = { entryOpen: false, entryStatus: "idle" as const, entryResult: undefined, entryError: undefined };

/** 出错时区分「没填 Key」（缓存命中时用不到 Key，所以出错后再查） */
async function failure(e: unknown): Promise<{ status: "error" | "nokey"; error?: string }> {
  const hasKey = await aiKeyStatus().catch(() => true);
  return hasKey ? { status: "error", error: String(e) } : { status: "nokey" };
}

export const useExpression = create<ExpressionState>((set, get) => ({
  key: null,
  status: "idle",
  ...noEntry,
  reset: () => set({ key: null, status: "idle", result: undefined, error: undefined, ...noEntry }),
  show: async (input, audience) => {
    const key = keyOf(input, audience);
    set({ key, status: "idle", result: undefined, error: undefined, ...noEntry });
    const hit = await expressionCached(input, audience).catch(() => null);
    if (get().key === key && get().status === "idle" && hit) set({ status: "done", result: hit });
  },
  run: async (input, audience, force = false) => {
    const key = keyOf(input, audience);
    if (get().key !== key) set(noEntry);
    set({ key, status: "loading", error: undefined, ...(force ? {} : { result: undefined }) });
    try {
      const result = await expressionGenerate(input, audience, force);
      if (get().key === key) set({ status: "done", result });
    } catch (e) {
      const f = await failure(e);
      if (get().key === key) set(f);
    }
  },
  openEntry: async (input, force = false) => {
    const key = get().key;
    set({ entryOpen: true, entryStatus: "loading", entryError: undefined, ...(force ? {} : { entryResult: undefined }) });
    try {
      const hit = force ? null : await expressionEntryCached(input).catch(() => null);
      const entryResult = hit ?? (await expressionEntryGenerate(input, force));
      if (get().key === key && get().entryOpen) set({ entryStatus: "done", entryResult });
    } catch (e) {
      const f = await failure(e);
      if (get().key === key && get().entryOpen) set({ entryStatus: f.status, entryError: f.error });
    }
  },
  closeEntry: () => set({ entryOpen: false }),
}));
