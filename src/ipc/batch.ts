// 批量导出命令与事件（SPEC 5.1、5.2、12）。

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ExportLayout, ExportOptions, ExportStyle, Paper } from "../export-templates/template";
import type { ContextRow } from "./history";
import type { RawEntry } from "./types";

export type BatchSource = "history" | "paste" | "wordbook";

export interface BatchRequest {
  source: BatchSource;
  /** 显示用说明，也是合并模式的文件名，如「2026-10-08_历史_86词」 */
  label: string;
  words: string[];
  pdf: boolean;
  html: boolean;
  merge: boolean;
  layout: ExportLayout;
  style: ExportStyle;
  paper: Paper;
  offlineOnly: boolean;
  outDir: string;
  options: ExportOptions;
  theme: string;
  accent: string;
  /** 界面语言：导出文件与批量导出提示用 */
  lang: "zh" | "en";
}

export interface ParseOutput {
  words: string[];
  total: number;
  merged: Array<{ from: string; to: string }>;
  invalid: Array<{ text: string; reason: string }>;
  overLimit: string[];
}

export interface Preflight {
  words: number;
  estSeconds: number;
  estBytes: number;
  freeBytes: number | null;
  writable: boolean;
  writeError: string | null;
  spaceOk: boolean;
  onlineDicts: number;
  onlineCached: number;
  onlineUncached: number;
}

export interface BatchCounts {
  total: number;
  pending: number;
  querying: number;
  rendering: number;
  done: number;
  failed: number;
  noted: number;
  files: number;
}

export type BatchState = "running" | "paused" | "cancelled" | "done" | "error";
export type ItemState = "pending" | "querying" | "rendering" | "done" | "failed" | "skipped";

export interface BatchItem {
  seq: number;
  lemma: string;
  state: ItemState;
  note: string | null;
  files: string[];
}

export interface BatchSummary {
  id: string;
  state: BatchState;
  label: string;
  outDir: string;
  pdf: boolean;
  html: boolean;
  merge: boolean;
  layout: ExportLayout;
  counts: BatchCounts;
  createdAt: number;
  updatedAt: number;
  active: boolean;
}

export interface BatchProgress {
  batchId: string;
  state: BatchState;
  /** 暂停命令发出的简短通知只带 batchId 和 state */
  partial?: boolean;
  counts?: BatchCounts;
  renderQueue?: number;
  memMb?: number;
  memLimitMb?: number;
  recycles?: number;
  startedAt?: number;
  changed?: BatchItem[];
  error?: string | null;
}

/** 导出窗口收到的渲染任务 */
export interface RenderJob {
  jobId: string;
  mode: "word" | "volume";
  title: string;
  words: Array<{ word: string; cards: Array<{ id: string; name: string; online: boolean; entries: RawEntry[] }>; context: ContextRow | null }>;
  layout: ExportLayout;
  style: ExportStyle;
  options: ExportOptions;
  theme: string;
  accent: string;
  paper: Paper;
  pdfPath: string | null;
  wantHtml: boolean;
  lang?: string;
}

export const batchParse = (text: string) => invoke<ParseOutput>("batch_parse", { text });
export const batchPreflight = (req: BatchRequest) => invoke<Preflight>("batch_preflight", { req });
export const startBatch = (req: BatchRequest) => invoke<string>("start_batch", { req });
export const pauseBatch = (id: string) => invoke("pause_batch", { id });
export const resumeBatch = (id: string) => invoke("resume_batch", { id });
export const cancelBatch = (id: string) => invoke("cancel_batch", { id });
export const retryFailedBatch = (id: string) => invoke<number>("retry_failed_batch", { id });
export const discardBatch = (id: string) => invoke<boolean>("discard_batch", { id });
export const listUnfinishedBatches = () => invoke<BatchSummary[]>("list_unfinished_batches");
export const batchStatus = (id: string) => invoke<BatchSummary>("batch_status", { id });
export const batchItems = (id: string, offset = 0, limit = 5000, filter = "all") =>
  invoke<BatchItem[]>("batch_items", { id, filter, offset, limit });

export function onBatchProgress(cb: (e: BatchProgress) => void): Promise<UnlistenFn> {
  return listen<BatchProgress>("batch:progress", (e) => cb(e.payload));
}
