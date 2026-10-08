// settings 相关命令的类型化封装（SPEC 5.1、14）。

import { invoke } from "@tauri-apps/api/core";

export interface Appearance {
  theme: string;
  followSystem: boolean;
  lightTheme: string;
  darkTheme: string;
  accent: string;
  glossFontSize: number;
  glossFont: "serif" | "sans";
}

export type LayoutPreset = "compact" | "two-column" | "three-column";

export interface Layout {
  preset: LayoutPreset;
  showNav: boolean;
  showSenseAtlas: boolean;
  showContext: boolean;
  showStatusBar: boolean;
  showJumpBar: boolean;
  cardsExpanded: "first2" | "all" | "none";
}

export interface Immersive {
  hideTitleBar: boolean;
  hideStatusBar: boolean;
  hideSidebars: boolean;
  /** 隐藏跳转条与复制按钮，卡片去边框只留小标题 */
  hideCardChrome: boolean;
  borderless: boolean;
  revealOnHover: boolean;
  fullscreen: boolean;
  width: 600 | 720 | 900;
  /** null = 跟随当前主题 */
  theme: string | null;
  onStartup: boolean;
}

export interface Capture {
  hotkey: string;
  doubleCopy: boolean;
  ocrFallback: boolean;
  timeoutMs: number;
}

export interface Online {
  /** 默认查询时机，每部词典可在词典库里覆盖 */
  trigger: "always" | "localMiss" | "manual";
  timeoutMs: number;
  /** 0 = 不缓存，-1 = 永久 */
  cacheDays: number;
}

export interface BatchPrefs {
  maxWords: number;
  lookupConcurrency: number;
  renderRecycleEvery: number;
  memLimitMb: number;
  mergeVolumeSize: number;
}

export interface Copy {
  defaultFormat: "md" | "html" | "text";
  appendSource: boolean;
  includeExamples: boolean;
  inlineHtmlStyles: boolean;
  mdImagesAsLinks: boolean;
}

export interface ExportPrefs {
  layout: "cards" | "document";
  style: "paperWhite" | "followTheme" | "inkSaving";
  paper: "a4" | "letter";
  dir: string | null;
}

/** 目前建模到 appearance、layout、immersive、capture、online、copy、export、batch；其他分组由 Rust 原样保留，接入时再补类型。 */
export interface Settings {
  appearance: Appearance;
  layout: Layout;
  immersive: Immersive;
  capture: Capture;
  online: Online;
  copy: Copy;
  export: ExportPrefs;
  batch: BatchPrefs;
  activeProfile: string;
  [section: string]: unknown;
}

export type DeepPartial<T> = { [K in keyof T]?: T[K] extends object ? DeepPartial<T[K]> : T[K] };

export function getSettings(): Promise<Settings> {
  return invoke<Settings>("get_settings");
}

/** 按 JSON Merge Patch 合并，返回合并后的完整设置。 */
export function setSettings(patch: DeepPartial<Settings>): Promise<Settings> {
  return invoke<Settings>("set_settings", { patch });
}

export interface AppPaths {
  data: string;
  settings: string;
  logs: string;
  version: string;
}

export function appPaths(): Promise<AppPaths> {
  return invoke<AppPaths>("app_paths");
}
