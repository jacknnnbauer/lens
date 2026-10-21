// settings 相关命令的类型化封装（SPEC 5.1、14）。

import { invoke } from "@tauri-apps/api/core";
import type { ExportStyle, Paper } from "../export-templates/template";
import type { CustomTheme } from "../theme/themes";

export interface Appearance {
  theme: string;
  followSystem: boolean;
  lightTheme: string;
  darkTheme: string;
  accent: string;
  glossFontSize: number;
  glossFont: "serif" | "sans";
  /** 界面语言 */
  language: "zh" | "en";
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
  /** 每次启动都回到简洁单栏 */
  startCompact: boolean;
  /** 侧栏宽度（px） */
  navWidth: number;
  contextWidth: number;
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
  /** Lens 窗口内查词：双击 / 单击 / 划词 / 关闭 */
  inAppTrigger: "dblclick" | "click" | "drag" | "off";
  /** 划词查到后自动收进生词本（连同语境句）；false = 手动收藏 */
  autoSave: boolean;
  /** 弹窗大小（逻辑像素），拖动边缘后由 Rust 记下 */
  popupWidth: number;
  popupHeight: number;
  /** 点关闭时隐藏到后台（托盘） */
  closeToTray: boolean;
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
  style: ExportStyle;
  paper: Paper;
  dir: string | null;
}

export type AiProvider = "claude" | "deepseek";

/** AI 双语例句（Claude 或 DeepSeek）；Key 存在凭据管理器，不在这里 */
export interface AiPrefs {
  provider: AiProvider;
  /** Claude 的模型 */
  model: string;
  /** DeepSeek 的模型 */
  deepseekModel: string;
  count: number;
  /** 查词后自动生成（否则点按钮） */
  auto: boolean;
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
  ai: AiPrefs;
  customThemes: CustomTheme[];
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

/** 新增或更新自定义主题（Rust 校验颜色格式），返回校验后的主题 */
export function saveCustomTheme(theme: CustomTheme): Promise<CustomTheme> {
  return invoke<CustomTheme>("save_custom_theme", { theme });
}

/** 删除自定义主题；正在使用它的设置回退到纸本 / 石墨。返回新的完整设置。 */
export function deleteCustomTheme(id: string): Promise<Settings> {
  return invoke<Settings>("delete_custom_theme", { id });
}

export function exportTheme(path: string, theme: CustomTheme): Promise<void> {
  return invoke("export_theme", { path, theme });
}

export function importTheme(path: string): Promise<CustomTheme> {
  return invoke<CustomTheme>("import_theme", { path });
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
