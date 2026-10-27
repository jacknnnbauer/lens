// 全文索引：反查（中文 → 英文词）、正文搜索、建索引（Rust：commands/fulltext.rs）。

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface FtStatus {
  dictId: string;
  name: string;
  state: "none" | "queued" | "building" | "ready";
  done: number;
  total: number;
  /** 索引文件大小（字节） */
  bytes: number;
  /** 词典文件大小（字节） */
  dictBytes: number;
}

export interface FtProgress {
  dictId: string;
  state: "building" | "ready" | "error" | "cancelled";
  done: number;
  total: number;
  error: string | null;
}

export interface ReverseWord {
  word: string;
  gloss: string;
  /** 3 = 释义就是这个词；2 = 以它开头或结尾；1 = 包含 */
  fit: number;
  dicts: string[];
  /** 词频排名（COCA 或内置词频表） */
  rank: number | null;
  pos: string | null;
}

export interface ReverseResult {
  words: ReverseWord[];
  /** 参与反查的（建了索引的）词典数 */
  indexed: number;
}

export interface Snippet {
  before: string;
  hit: string;
  after: string;
}

export interface FtGroup {
  dictId: string;
  name: string;
  total: number;
  hits: Array<{ headword: string; snippet: Snippet | null }>;
}

export function fulltextStatus(): Promise<FtStatus[]> {
  return invoke<FtStatus[]>("fulltext_status");
}

export function fulltextBuild(dictIds: string[]): Promise<void> {
  return invoke("fulltext_build", { dictIds });
}

export function fulltextCancel(): Promise<void> {
  return invoke("fulltext_cancel");
}

export function fulltextDelete(dictId: string): Promise<void> {
  return invoke("fulltext_delete", { dictId });
}

export function reverseLookup(query: string, limit?: number): Promise<ReverseResult> {
  return invoke<ReverseResult>("reverse_lookup", { query, limit: limit ?? null });
}

/** dictId 给定时只搜这一部（翻页） */
export function fulltextSearch(query: string, dictId?: string, offset?: number): Promise<FtGroup[]> {
  return invoke<FtGroup[]>("fulltext_search", { query, dictId: dictId ?? null, offset: offset ?? null });
}

export function onFulltextProgress(cb: (e: FtProgress) => void): Promise<UnlistenFn> {
  return listen<FtProgress>("fulltext:progress", (e) => cb(e.payload));
}

/** 含汉字：按中文反查 */
export function hasCjk(s: string): boolean {
  return /[㐀-䶿一-鿿豈-﫿]/.test(s);
}
