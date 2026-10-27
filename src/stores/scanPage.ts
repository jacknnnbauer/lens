// 文章生词扫描页的文章和结果：切到别的页面（查词）再回来时还在。

import { create } from "zustand";
import type { ScanResult } from "../ipc/scan";

export const useScanPage = create<{
  text: string;
  title: string;
  result: ScanResult | null;
  setText: (text: string) => void;
  setTitle: (title: string) => void;
  setResult: (result: ScanResult | null) => void;
}>((set) => ({
  text: "",
  title: "",
  result: null,
  setText: (text) => set({ text }),
  setTitle: (title) => set({ title }),
  setResult: (result) => set({ result }),
}));
