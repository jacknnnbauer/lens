// settings.json 的前端镜像：改动立即生效，300ms 防抖后写回 Rust。

import { create } from "zustand";
import { getSettings, setSettings, type Appearance, type DeepPartial, type Settings } from "../ipc/settings";

const SAVE_DEBOUNCE_MS = 300;

interface SettingsState {
  settings: Settings | null;
  error: string | null;
  load: () => Promise<void>;
  updateAppearance: (patch: Partial<Appearance>) => void;
}

let pending: DeepPartial<Settings> = {};
let timer: ReturnType<typeof setTimeout> | undefined;

export const useSettings = create<SettingsState>((set, get) => {
  const flush = async () => {
    const patch = pending;
    pending = {};
    try {
      await setSettings(patch);
      set({ error: null });
    } catch (e) {
      set({ error: String(e) });
    }
  };

  return {
    settings: null,
    error: null,

    load: async () => {
      try {
        set({ settings: await getSettings(), error: null });
      } catch (e) {
        set({ error: String(e) });
      }
    },

    updateAppearance: (patch) => {
      const current = get().settings;
      if (!current) return;
      set({ settings: { ...current, appearance: { ...current.appearance, ...patch } } });
      pending = { ...pending, appearance: { ...pending.appearance, ...patch } };
      clearTimeout(timer);
      timer = setTimeout(flush, SAVE_DEBOUNCE_MS);
    },
  };
});
