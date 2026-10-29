// 阅读页：当前打开的文章（切到别的页面再回来时还在这篇），以及主窗口右侧栏里给阅读页留的位置。

import { create } from "zustand";

interface ReaderState {
  openId: number | null;
  open: (id: number | null) => void;
  /** 右侧栏打开时由主窗口挂上，阅读页把释义 / 生词渲染进去；关着时为 null */
  sideHost: HTMLElement | null;
  setSideHost: (el: HTMLElement | null) => void;
}

export const useReader = create<ReaderState>((set) => ({
  openId: null,
  open: (openId) => set({ openId }),
  sideHost: null,
  setSideHost: (sideHost) => set({ sideHost }),
}));
