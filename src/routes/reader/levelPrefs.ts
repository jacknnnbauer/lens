// 阅读的生词门槛：默认值、从旧版本地存储迁移，以及下拉框选择后的结果。

export const DEFAULT_LEVEL = 5000;
export const MAX_LEVEL = 40000;

export interface LevelPref {
  level: number;
  follow: boolean;
}

/** 旧版把门槛存在 localStorage：第一次读到时搬进设置，并保留用户手动选的值 */
export function migrateLevel(saved: LevelPref, legacy: unknown): LevelPref | null {
  if (saved.level > 0) return null;
  if (typeof legacy !== "number" || !Number.isInteger(legacy) || legacy < 1 || legacy > MAX_LEVEL) return null;
  return { level: legacy, follow: false };
}

export function effectiveLevel(level: number): number {
  return level > 0 ? level : DEFAULT_LEVEL;
}

/** 下拉框的选择：`me` = 我的词汇量（跟随测试结果），数字 = 固定门槛 */
export function chooseLevel(choice: "me" | number, estimate: number): LevelPref {
  if (choice === "me") return { level: estimate, follow: true };
  return { level: Math.min(MAX_LEVEL, Math.max(1, Math.round(choice))), follow: false };
}
