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

/** 目前只建模到 appearance；其他分组由 Rust 原样保留，接入时再补类型。 */
export interface Settings {
  appearance: Appearance;
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
