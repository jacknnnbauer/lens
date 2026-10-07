// 查词相关命令与事件（SPEC 5.1、5.2）。

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { LookupDoneEvent, LookupHandle, LookupResultEvent } from "./types";

/** 立即返回；结果通过 onLookupResult 逐部推送，最后 onLookupDone。 */
export function lookup(query: string): Promise<LookupHandle> {
  return invoke<LookupHandle>("lookup", { query });
}

export function cancelLookup(requestId: string): Promise<void> {
  return invoke("cancel_lookup", { requestId });
}

export function suggest(prefix: string, limit = 8): Promise<string[]> {
  return invoke<string[]>("suggest", { prefix, limit });
}

export function onLookupResult(cb: (e: LookupResultEvent) => void): Promise<UnlistenFn> {
  return listen<LookupResultEvent>("lookup:result", (e) => cb(e.payload));
}

export function onLookupDone(cb: (e: LookupDoneEvent) => void): Promise<UnlistenFn> {
  return listen<LookupDoneEvent>("lookup:done", (e) => cb(e.payload));
}
