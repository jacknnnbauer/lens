// 主窗口页面（SPEC 9.1 的 /lookup、/history…）。窗口内只有一个页面栈，不需要 URL 路由。

import { create } from "zustand";

export type Page = "lookup" | "history" | "wordbook" | "settings" | "themes";

export const useRoute = create<{ page: Page; go: (page: Page) => void }>((set) => ({
  page: "lookup",
  go: (page) => set({ page }),
}));
