// 在线词典命令（SPEC 13）。

import { invoke } from "@tauri-apps/api/core";
import type { LookupResultEvent, OnlineOptions, ProviderInfo } from "./types";

export function listOnlineProviders(): Promise<ProviderInfo[]> {
  return invoke<ProviderInfo[]>("list_online_providers");
}

/** 返回新词典的 id */
export function addOnlineDict(provider: string, name?: string, urlTemplate?: string): Promise<string> {
  return invoke<string>("add_online_dict", { provider, name: name ?? null, urlTemplate: urlTemplate ?? null });
}

export function updateOnlineDict(id: string, options: OnlineOptions, name?: string): Promise<void> {
  return invoke("update_online_dict", { id, name: name ?? null, options });
}

/** key 为空表示清除；返回现在是否有 Key */
export function setOnlineKey(id: string, key: string, appId?: string): Promise<boolean> {
  return invoke<boolean>("set_online_key", { id, key, appId: appId ?? null });
}

/** 手动查询一部在线词典 */
export function lookupOnline(requestId: string, id: string, query: string): Promise<LookupResultEvent> {
  return invoke<LookupResultEvent>("lookup_online", { requestId, id, query });
}

export function openWebDict(url: string, title?: string): Promise<void> {
  return invoke("open_web_dict", { url, title: title ?? null });
}
