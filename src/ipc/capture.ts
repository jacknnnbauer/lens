// 取词与弹窗（SPEC 5.2、8）。

import { invoke } from "@tauri-apps/api/core";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

export interface ContextSentence {
  sentence: string;
  /** 选区在 sentence 中的 UTF-16 偏移（JS 字符串下标可直接用） */
  hlStart: number;
  hlEnd: number;
  before: string[];
  after: string[];
}

export interface CaptureSource {
  method: "uia" | "clipboard" | "manual";
  app: string | null;
  windowTitle: string | null;
  url: string | null;
}

export interface CaptureEvent {
  id: number;
  text: string;
  /** 已存入数据库的语境句 id */
  contextId: number | null;
  sentence: ContextSentence | null;
  source: CaptureSource;
  ms: number;
}

export function onCapture(cb: (e: CaptureEvent) => void): Promise<UnlistenFn> {
  return getCurrentWebviewWindow().listen<CaptureEvent>("capture:triggered", (e) => cb(e.payload));
}

/** 浏览器地址栏 URL 取得较慢，弹窗显示后补发 */
export function onCaptureUrl(cb: (e: { id: number; url: string }) => void): Promise<UnlistenFn> {
  return getCurrentWebviewWindow().listen<{ id: number; url: string }>("capture:url", (e) => cb(e.payload));
}

/** 主窗口：弹窗里按 Enter 后要查的词 */
export function onMainLookup(cb: (query: string) => void): Promise<UnlistenFn> {
  return getCurrentWebviewWindow().listen<string>("main:lookup", (e) => cb(e.payload));
}

export function hidePopup(): Promise<void> {
  return invoke("hide_popup");
}

export function openInMain(query: string): Promise<void> {
  return invoke("open_in_main", { query });
}

export interface CaptureStatus {
  hotkey: string;
  hotkeyRegistered: boolean;
  hotkeyError: string | null;
  doubleCopy: boolean;
}

export function captureStatus(): Promise<CaptureStatus> {
  return invoke<CaptureStatus>("capture_status");
}

/** 改快捷键后后端重新注册，结果推给所有窗口 */
export function onCaptureStatus(cb: (s: CaptureStatus) => void): Promise<UnlistenFn> {
  return getCurrentWebviewWindow().listen<CaptureStatus>("capture:status", (e) => cb(e.payload));
}
