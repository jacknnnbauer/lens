// 英文怎么说（Rust：commands/ai.rs · expression_ai.rs）

import { invoke } from "@tauri-apps/api/core";

export interface ExpressionItem {
  /** 简单 / 地道 / 母语级 / 幽默 / 含蓄 */
  style: string;
  en: string;
  /** 一句中文说明 */
  note: string;
}

export interface ExpressionResult {
  expression: { items: ExpressionItem[] };
  model: string;
  /** 缓存里取的：当时生成的时间；刚生成的为 null */
  cachedAt: number | null;
  costUsd: number;
}

/** 生成五种说法（先查缓存；force 时重新生成） */
export const expressionGenerate = (input: string, audience: string | null, force: boolean) =>
  invoke<ExpressionResult>("expression_generate", { input, audience, force });

/** 只查缓存，不联网 */
export const expressionCached = (input: string, audience: string | null) => invoke<ExpressionResult | null>("expression_cached", { input, audience });
