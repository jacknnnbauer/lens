// AI 双语例句的长度与句型（和 Rust 的 ai::Style、ai::CLAUSES 一致）。

export type ExampleLength = "short" | "medium" | "long";

export const LENGTHS: Array<[ExampleLength, string]> = [
  ["short", "短（5–10 词）"],
  ["medium", "适中（8–22 词）"],
  ["long", "长（20–35 词）"],
];

/** 15 种从句：id 和 Rust 一致，name 是界面上的叫法 */
export const CLAUSES = [
  { id: "subject", name: "主语从句", group: "名词性从句" },
  { id: "object", name: "宾语从句", group: "名词性从句" },
  { id: "predicative", name: "表语从句", group: "名词性从句" },
  { id: "appositive", name: "同位语从句", group: "名词性从句" },
  { id: "restrictive", name: "限定性定语从句", group: "定语从句" },
  { id: "nonrestrictive", name: "非限定性定语从句", group: "定语从句" },
  { id: "time", name: "时间状语从句", group: "状语从句" },
  { id: "place", name: "地点状语从句", group: "状语从句" },
  { id: "reason", name: "原因状语从句", group: "状语从句" },
  { id: "condition", name: "条件状语从句", group: "状语从句" },
  { id: "purpose", name: "目的状语从句", group: "状语从句" },
  { id: "result", name: "结果状语从句", group: "状语从句" },
  { id: "concession", name: "让步状语从句", group: "状语从句" },
  { id: "comparison", name: "比较状语从句", group: "状语从句" },
  { id: "manner", name: "方式状语从句", group: "状语从句" },
] as const;

export const CLAUSE_GROUPS = ["名词性从句", "定语从句", "状语从句"] as const;

export function clauseName(id: string): string | null {
  return CLAUSES.find((c) => c.id === id)?.name ?? null;
}

/** 规整：未知句型丢掉、按清单顺序去重 */
export function normalizeClauses(ids: readonly string[] | undefined): string[] {
  const set = new Set(ids ?? []);
  return CLAUSES.filter((c) => set.has(c.id)).map((c) => c.id);
}

/** 缓存用的样式键：适中且不限句型为空串（和旧缓存一致） */
export function styleKey(p: { length?: string; clauses?: readonly string[] } | undefined): string {
  const length = p?.length === "short" || p?.length === "long" ? p.length : "medium";
  const clauses = normalizeClauses(p?.clauses);
  if (length === "medium" && clauses.length === 0) return "";
  return `${length}|${clauses.join(",")}`;
}
