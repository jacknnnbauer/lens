// 查词相关命令与事件（SPEC 5.1、5.2）。

import { invoke } from "@tauri-apps/api/core";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
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

// 查询事件由后端只发给发起查询的窗口，这里也只监听本窗口（主窗口与弹窗各查各的）

export function onLookupResult(cb: (e: LookupResultEvent) => void): Promise<UnlistenFn> {
  return getCurrentWebviewWindow().listen<LookupResultEvent>("lookup:result", (e) => cb(e.payload));
}

export function onLookupDone(cb: (e: LookupDoneEvent) => void): Promise<UnlistenFn> {
  return getCurrentWebviewWindow().listen<LookupDoneEvent>("lookup:done", (e) => cb(e.payload));
}
