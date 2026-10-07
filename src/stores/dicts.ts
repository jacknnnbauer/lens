// 词典库：列表、导入进度、删除。

import { create } from "zustand";
import { importDicts, listDicts, listProfiles, onIndexProgress, removeDict } from "../ipc/dicts";
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

  remove: async (id) => {
    try {
      await removeDict(id);
      await get().refresh();
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
