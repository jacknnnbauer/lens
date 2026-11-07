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
/** `dict`：先查这部词典，查不到再按生词本设置和配置组的顺序；`alts`：原形查不到时再查的写法（书里的变形） */
export function firstEntry(query: string, dict?: string | null, alts?: string[]): Promise<FirstEntry | null> {
  return invoke<FirstEntry | null>("first_entry", { query, dict: dict ?? null, alts: alts ?? null });
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

export interface WordFamily {
  word: string;
  stem: string;
  root: string | null;
  /** kind：derived 派生词 | root 同根词 | confusable 易混词 | synonym 近义词 */
  groups: Array<{ kind: string; words: string[] }>;
}

/** 词汇网络（离线）：派生词、同根词、易混词、近义词 */
export interface WordGloss {
  word: string;
  pos: string | null;
  gloss: string | null;
}

/** 词汇网络表格视图：每个词的词性和第一段中文释义 */
export interface SenseGloss {
  word: string;
  pos: string | null;
  /** 各义项的中文（最多 4 个） */
  senses: string[];
  /** 取自哪部词典 */
  dict: string | null;
}

/** 全书预习：每个词各义项的中文，先查 `dict`；原形查不到时查 `forms`（书里的写法）。一次最多 200 个 */
export function senseGlosses(words: Array<{ lemma: string; forms: string[] }>, dict: string | null): Promise<SenseGloss[]> {
  return invoke<SenseGloss[]>("sense_glosses", { words, dict });
}

export function wordGlosses(words: string[]): Promise<WordGloss[]> {
  return invoke<WordGloss[]>("word_glosses", { words });
}

export function wordFamily(query: string): Promise<WordFamily> {
  return invoke<WordFamily>("word_family", { query });
}
