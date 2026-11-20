// 英文怎么说：当前这条（输入 + 对象描述）的状态。结果由 Rust 缓存，这里只管显示；
// 请求还没回来时换了输入，旧结果回来就丢掉。

import { create } from "zustand";
import { aiKeyStatus } from "../ipc/ai";
import { expressionCached, expressionGenerate, type ExpressionResult } from "../ipc/expression";

export type ExpressionStatus = "idle" | "loading" | "done" | "error" | "nokey";

interface ExpressionState {
  key: string | null;
  status: ExpressionStatus;
  result?: ExpressionResult;
  error?: string;
  /** 输入或对象变了：有缓存就直接显示（不花钱），没有就回到折叠态 */
  show: (input: string, audience: string | null) => Promise<void>;
  /** 点开 / 换一批：生成（命令先查缓存，force 时重新生成） */
  run: (input: string, audience: string | null, force?: boolean) => Promise<void>;
  reset: () => void;
}

const keyOf = (input: string, audience: string | null) => `${input}\u0000${audience ?? ""}`;

export const useExpression = create<ExpressionState>((set, get) => ({
  key: null,
  status: "idle",
  reset: () => set({ key: null, status: "idle", result: undefined, error: undefined }),
  show: async (input, audience) => {
    const key = keyOf(input, audience);
    set({ key, status: "idle", result: undefined, error: undefined });
    const hit = await expressionCached(input, audience).catch(() => null);
    if (get().key === key && get().status === "idle" && hit) set({ status: "done", result: hit });
  },
  run: async (input, audience, force = false) => {
    const key = keyOf(input, audience);
    set({ key, status: "loading", error: undefined, ...(force ? {} : { result: undefined }) });
    try {
      const result = await expressionGenerate(input, audience, force);
      if (get().key === key) set({ status: "done", result });
    } catch (e) {
      // 没填 Key：给去设置的提示（缓存命中时用不到 Key，所以出错后再查）
      const hasKey = await aiKeyStatus().catch(() => true);
      if (get().key === key) set(hasKey ? { status: "error", error: String(e) } : { status: "nokey" });
    }
  },
}));
