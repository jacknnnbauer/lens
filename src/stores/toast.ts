// 轻提示（画板 05 底部的「已复制…」）：同一时间只显示一条，几秒后消失。
// 可以带一个按钮（例如「撤销」）：带按钮时停留久一点，点了就执行并关掉。

import { create } from "zustand";

const SHOW_MS = 2600;
const ACTION_MS = 5000;
let timer: ReturnType<typeof setTimeout> | undefined;

export interface ToastAction {
  label: string;
  run: () => void;
}

export const useToast = create<{ text: string | null; note: string | null; action: ToastAction | null }>(() => ({ text: null, note: null, action: null }));

function hide() {
  useToast.setState({ text: null, note: null, action: null });
}

export function toast(text: string, note: string | null = null, action: ToastAction | null = null) {
  clearTimeout(timer);
  useToast.setState({ text, note, action });
  timer = setTimeout(hide, action ? ACTION_MS : SHOW_MS);
}

/** 点了提示上的按钮 */
export function runToastAction() {
  const a = useToast.getState().action;
  clearTimeout(timer);
  hide();
  a?.run();
}
