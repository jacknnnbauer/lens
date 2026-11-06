// 词典库：列表、导入进度、删除。

import { create } from "zustand";
import { importDicts, listDicts, listProfiles, onIndexProgress, removeDict, reorderDicts, setProfileDict, relinkDict } from "../ipc/dicts";
import { useLookup } from "./lookup";
import type { DictInfo, IndexProgress, ProfileInfo } from "../ipc/types";

interface DictsState {
  dicts: DictInfo[];
  profiles: ProfileInfo[];
  loaded: boolean;
  /** 本次会话里的导入记录，按文件路径 */
  imports: Record<string, IndexProgress>;
  error: string | null;
  refresh: () => Promise<void>;
  importPaths: (paths: string[]) => Promise<void>;
  remove: (id: string) => Promise<void>;
  /** 词典文件挪了位置：重新指定文件 */
  relink: (id: string, path: string) => Promise<void>;
  /** 上移（-1）/ 下移（+1） */
  move: (id: string, dir: -1 | 1) => Promise<void>;
  /** 按新顺序排列（拖动把手）；当前查询随即按新顺序重查 */
  setOrder: (ids: string[]) => Promise<void>;
  /** 只写回顺序，不重新查词（卡片墙拖动换位：结果已经在界面上换好了） */
  setOrderQuiet: (ids: string[]) => Promise<void>;
  /** 在场景配置组里启用 / 停用词典；改的是当前配置组时，当前查询随即重查 */
  setInProfile: (profile: string, dict: string, enabled: boolean, active: boolean) => Promise<void>;
}

export const useDicts = create<DictsState>((set, get) => ({
  dicts: [],
  profiles: [],
  loaded: false,
  imports: {},
  error: null,

  refresh: async () => {
    try {
      const [dicts, profiles] = await Promise.all([listDicts(), listProfiles()]);
      set({ dicts, profiles, loaded: true, error: null });
    } catch (e) {
      set({ error: String(e), loaded: true });
    }
  },

  importPaths: async (paths) => {
    if (paths.length === 0) return;
    try {
      await importDicts(paths);
    } catch (e) {
      set({ error: String(e) });
    }
  },

  move: async (id, dir) => {
    const ids = get().dicts.map((d) => d.id);
    const i = ids.indexOf(id);
    const j = i + dir;
    if (i < 0 || j < 0 || j >= ids.length) return;
    [ids[i], ids[j]] = [ids[j], ids[i]];
    await get().setOrder(ids);
  },

  setOrder: async (ids) => {
    // 先在界面上换位，再写库
    const byId = new Map(get().dicts.map((d) => [d.id, d]));
    set({ dicts: ids.flatMap((x) => byId.get(x) ?? []) });
    try {
      await reorderDicts(ids);
      await get().refresh();
      await useLookup.getState().rerun();
    } catch (e) {
      set({ error: String(e) });
      await get().refresh();
    }
  },

  setOrderQuiet: async (ids) => {
    const byId = new Map(get().dicts.map((d) => [d.id, d]));
    set({ dicts: ids.flatMap((x) => byId.get(x) ?? []) });
    try {
      await reorderDicts(ids);
    } catch (e) {
      set({ error: String(e) });
      await get().refresh();
    }
  },

  setInProfile: async (profile, dict, enabled, active) => {
    // 先在界面上勾好，再写库
    set((s) => ({
      dicts: s.dicts.map((d) =>
        d.id === dict ? { ...d, profiles: enabled ? [...d.profiles.filter((p) => p !== profile), profile] : d.profiles.filter((p) => p !== profile) } : d,
      ),
    }));
    try {
      await setProfileDict(profile, dict, enabled);
      await get().refresh();
      if (active) await useLookup.getState().rerun();
    } catch (e) {
      set({ error: String(e) });
      await get().refresh();
    }
  },

  remove: async (id) => {
    try {
      await removeDict(id);
      await get().refresh();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  relink: async (id, path) => {
    try {
      set({ error: null });
      await relinkDict(id, path);
      await get().refresh();
      await useLookup.getState().rerun();
    } catch (e) {
      set({ error: String(e) });
    }
  },
}));

export async function subscribeDictEvents(): Promise<() => void> {
  return onIndexProgress((p) => {
    useDicts.setState((s) => ({ imports: { ...s.imports, [p.path]: p } }));
    if (p.phase !== "start") void useDicts.getState().refresh();
  });
}

export function fileName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}
