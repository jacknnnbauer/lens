// settings.json 的前端镜像：改动立即生效，300ms 防抖后写回 Rust。

import { create } from "zustand";
import { getSettings, setSettings, type Appearance, type DeepPartial, type Layout, type Settings } from "../ipc/settings";

const SAVE_DEBOUNCE_MS = 300;

interface SettingsState {
  settings: Settings | null;
  error: string | null;
  load: () => Promise<void>;
  /** 防抖保存 */
  update: (patch: DeepPartial<Settings>) => void;
  /** 立即保存（后端马上要读到新值时用，如切换场景配置组后重新查词） */
  updateNow: (patch: DeepPartial<Settings>) => Promise<void>;
  updateAppearance: (patch: Partial<Appearance>) => void;
  updateLayout: (patch: Partial<Layout>) => void;
}

/** 两层浅合并就够用：设置只有「分组 → 字段」两层 */
function merge<T extends object>(base: T, patch: DeepPartial<T>): T {
  const out: Record<string, unknown> = { ...(base as Record<string, unknown>) };
  for (const [k, v] of Object.entries(patch)) {
    const cur = out[k];
    out[k] =
      v && typeof v === "object" && !Array.isArray(v) && cur && typeof cur === "object"
        ? { ...(cur as object), ...(v as object) }
        : v;
  }
  return out as T;
}

let pending: DeepPartial<Settings> = {};
let timer: ReturnType<typeof setTimeout> | undefined;

export const useSettings = create<SettingsState>((set, get) => {
  const flush = async () => {
    clearTimeout(timer);
    const patch = pending;
    pending = {};
    if (Object.keys(patch).length === 0) return;
    try {
      await setSettings(patch);
      set({ error: null });
    } catch (e) {
      set({ error: String(e) });
    }
  };

  const apply = (patch: DeepPartial<Settings>) => {
    const current = get().settings;
    if (!current) return false;
    set({ settings: merge(current, patch) });
    pending = merge(pending as Settings, patch);
    return true;
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

    update: (patch) => {
      if (!apply(patch)) return;
      clearTimeout(timer);
      timer = setTimeout(() => void flush(), SAVE_DEBOUNCE_MS);
    },

    updateNow: async (patch) => {
      if (apply(patch)) await flush();
    },

    updateAppearance: (patch) => get().update({ appearance: patch }),
    updateLayout: (patch) => get().update({ layout: patch }),
  };
});
