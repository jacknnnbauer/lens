// 查词相关命令与事件（SPEC 5.1、5.2）。

import { invoke } from "@tauri-apps/api/core";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { EntryBody, LookupDoneEvent, LookupHandle, LookupResultEvent } from "./types";

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

// —— 义项图谱（M8 实验） ——

export interface AtlasSense {
  /** 词典在 dicts 里的下标 */
  dict: number;
  /** 在该词典里的顺序 */
  index: number;
  text: string;
  pos: string | null;
  domain: string | null;
}

export interface AtlasCluster {
  label: string;
  senses: AtlasSense[];
  dicts: number[];
  domain: string | null;
  /** 成员词性不一致时：[词典下标, 词性] */
  posDiff: Array<[number, string]>;
}

export interface SenseAtlas {
  dicts: Array<{ id: string; name: string; senses: number }>;
  clusters: AtlasCluster[];
}

export function senseAtlas(word: string, dicts: Array<{ id: string; name: string; entries: string[] }>): Promise<SenseAtlas> {
  return invoke<SenseAtlas>("sense_atlas", { word, dicts });
}

export interface FirstEntry {
  dictId: string;
  name: string;
  entries: Array<{ headword: string; body: EntryBody }>;
}

/** 第一本命中的本地词典的词条（只查本地，不影响当前查询、不记历史）：语境卡背面用 */
export function firstEntry(query: string): Promise<FirstEntry | null> {
  return invoke<FirstEntry | null>("first_entry", { query });
}

export interface SenseCandidate {
  label: string;
  pos: string | null;
  dicts: string[];
  firstDict: string;
  senseNo: number;
  score: number;
}

export interface ContextSenseResult {
  surface: string;
  pos: string | null;
  best: SenseCandidate | null;
  others: SenseCandidate[];
  reasons: string[];
  ruledOut: string[];
  confident: boolean;
}

/** 语境释义（本地）：这句话里的这个词是哪个意思、依据（UTF-16 高亮区间） */
export function contextSense(input: {
  word: string;
  sentence: string;
  hlStart: number;
  hlEnd: number;
  dicts: Array<{ id: string; name: string; entries: string[] }>;
}): Promise<ContextSenseResult> {
  return invoke<ContextSenseResult>("context_sense", { input });
}
