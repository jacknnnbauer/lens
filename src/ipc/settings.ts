// settings 相关命令的类型化封装（SPEC 5.1、14）。

import { invoke } from "@tauri-apps/api/core";

/** 主窗口画好第一帧，请后端显示窗口 */
export const mainReady = () => invoke<void>("main_ready").catch(() => undefined);
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
  /** 查词结果上方的「当前语境」 */
  showContextSense: boolean;
  /** 查词结果里的「词汇网络」 */
  showWordFamily: boolean;
  /** 词汇网络：放射图 / 表格 */
  wordFamilyView: "graph" | "table";
  cardsExpanded: "first2" | "all" | "none";
  /** 每次启动都回到简洁单栏 */
  startCompact: boolean;
  /** 侧栏宽度（px） */
  navWidth: number;
  contextWidth: number;
  /** 右侧栏内容与顺序：coca | vocab | ai | query | dicts | dict:<词典 id> */
  panel: PanelItem[];
  /** 卡片墙（瀑布流）：卡片按自身高度铺满多列 */
  cardWall: boolean;
  /** 每列最小宽度（px） */
  wallColumn: number;
  /** 单张卡片最大高度（px），0 = 不限 */
  wallMaxHeight: number;
}

export interface PanelItem {
  id: string;
  show: boolean;
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
}

/** 生词本 */
export interface WordbookPrefs {
  /** 语境卡 / 复习卡背面用哪本词典（词典 id）；空 = 自动 */
  cardDict: string;
  /** 主窗口查词有结果就自动收藏 */
  autoSaveLookup: boolean;
}

/** 常规：整个应用的行为 */
export interface General {
  /** 启动时检查新版本（每天最多一次） */
  checkUpdates: boolean;
  lastUpdateCheck: number;
  /** 点关闭按钮时隐藏到托盘（Lens 留在后台）；false = 退出 */
  closeToTray: boolean;
  /** 第一次隐藏到托盘时已提示过 */
  trayHintShown: boolean;
  /** 全新安装的第一次启动已经过了 */
  introDone: boolean;
  /** 看过的一次性提示 */
  hintsSeen: string[];
}

export interface Online {
  /** 默认查询时机，每部词典可在词典库里覆盖 */
  trigger: "always" | "localMiss" | "manual";
  timeoutMs: number;
  /** 0 = 不缓存，-1 = 永久 */
  cacheDays: number;
  /** vocabulary.com 真实例句（非官方接口） */
  vocabExamples: boolean;
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
  general: General;
  wordbook: WordbookPrefs;
  appearance: Appearance;
  layout: Layout;
  immersive: Immersive;
  capture: Capture;
  online: Online;
  copy: Copy;
  export: ExportPrefs;
  batch: BatchPrefs;
  ai: AiPrefs;
  study: StudyPrefs;
  customThemes: CustomTheme[];
  activeProfile: string;
  [section: string]: unknown;
}

/** 全书预习包 */
export interface StudyPrefs {
  /** 读前导读里放情节梗概和人物（会剧透结局） */
  spoilers: boolean;
  /** zh = 有中文条目就用中文维基，en = 英文维基 */
  guideLang: "zh" | "en";
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
  portable: boolean;
}

export function appPaths(): Promise<AppPaths> {
  return invoke<AppPaths>("app_paths");
}

export interface UpdateInfo {
  current: string;
  /** 还没有公开发布时为 null */
  latest: string | null;
  newer: boolean;
  url: string | null;
  notes: string | null;
}

/** auto：启动时自动检查（关了或一天内查过返回 null，失败也返回 null） */
export function checkUpdate(auto: boolean): Promise<UpdateInfo | null> {
  return invoke<UpdateInfo | null>("check_update", { auto });
}
