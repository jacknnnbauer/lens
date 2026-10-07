// 词典库命令与导入进度事件（SPEC 5.1、5.2）。

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { DictInfo, IndexProgress } from "./types";

export function listDicts(): Promise<DictInfo[]> {
  return invoke<DictInfo[]>("list_dicts");
}

/** 后台依次导入，立即返回；进度见 onIndexProgress。 */
export function importDicts(paths: string[]): Promise<void> {
  return invoke("import_dicts", { paths });
}

export function removeDict(id: string): Promise<boolean> {
  return invoke<boolean>("remove_dict", { id });
}

export function onIndexProgress(cb: (e: IndexProgress) => void): Promise<UnlistenFn> {
  return listen<IndexProgress>("dict:index-progress", (e) => cb(e.payload));
}
