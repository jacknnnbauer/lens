// 阅读页：当前打开的文章（切到别的页面再回来时还在这篇）。

import { create } from "zustand";

export const useReader = create<{ openId: number | null; open: (id: number | null) => void }>((set) => ({
  openId: null,
  open: (openId) => set({ openId }),
}));
