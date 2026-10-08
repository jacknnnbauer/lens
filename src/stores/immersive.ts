// 沉浸模式开关（SPEC 9.2）。只存在于主窗口；具体隐藏哪些区域、窗口怎么变由设置 › 沉浸模式决定。

import { create } from "zustand";

interface ImmersiveState {
  on: boolean;
  enter: () => void;
  exit: () => void;
  toggle: () => void;
}

export const useImmersive = create<ImmersiveState>((set, get) => ({
  on: false,
  enter: () => set({ on: true }),
  exit: () => set({ on: false }),
  toggle: () => set({ on: !get().on }),
}));
