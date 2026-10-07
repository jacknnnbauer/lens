// 轻提示（画板 05 底部的「已复制…」）：同一时间只显示一条，几秒后消失。

import { create } from "zustand";

const SHOW_MS = 2600;
let timer: ReturnType<typeof setTimeout> | undefined;

export const useToast = create<{ text: string | null; note: string | null }>(() => ({ text: null, note: null }));

export function toast(text: string, note: string | null = null) {
  clearTimeout(timer);
  useToast.setState({ text, note });
  timer = setTimeout(() => useToast.setState({ text: null, note: null }), SHOW_MS);
}
