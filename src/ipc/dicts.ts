// 词典库命令与导入进度事件（SPEC 5.1、5.2）。

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { DictInfo, IndexProgress, ProfileInfo } from "./types";

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

/** 调整词典顺序（所有场景配置组都按这个顺序） */
export function reorderDicts(ids: string[]): Promise<void> {
  return invoke("reorder_dicts", { ids });
}

/** 在某个场景配置组里启用 / 停用一部词典 */
export function setProfileDict(profile: string, dict: string, enabled: boolean): Promise<boolean> {
  return invoke<boolean>("set_profile_dict", { profile, dict, enabled });
}

export function listProfiles(): Promise<ProfileInfo[]> {
  return invoke<ProfileInfo[]>("list_profiles");
}

export function onIndexProgress(cb: (e: IndexProgress) => void): Promise<UnlistenFn> {
  return listen<IndexProgress>("dict:index-progress", (e) => cb(e.payload));
}
