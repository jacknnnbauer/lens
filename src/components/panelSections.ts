// 右侧栏的内容：内置几块 + 用户加进来的词典（dict:<id>）。设置 › 布局里勾选、排序。

import type { PanelItem } from "../ipc/settings";

export const BUILTIN_SECTIONS = [
  { id: "coca", name: "COCA 词频与搭配" },
  { id: "vocab", name: "真实例句（vocabulary.com）" },
  { id: "ai", name: "AI 双语例句" },
  { id: "query", name: "查询" },
  { id: "dicts", name: "各词典命中情况" },
] as const;

export const DICT_PREFIX = "dict:";

export function dictIdOf(itemId: string): string | null {
  return itemId.startsWith(DICT_PREFIX) ? itemId.slice(DICT_PREFIX.length) : null;
}

/** 整理设置里的列表：去重、去掉不认识的项；以后新增的内置项补在末尾（默认显示）。 */
export function normalizePanel(list: PanelItem[] | undefined): PanelItem[] {
  const known = new Set<string>(BUILTIN_SECTIONS.map((s) => s.id));
  const out: PanelItem[] = [];
  const seen = new Set<string>();
  for (const it of list ?? []) {
    if (seen.has(it.id) || !(known.has(it.id) || (dictIdOf(it.id) ?? "") !== "")) continue;
    seen.add(it.id);
    out.push({ id: it.id, show: it.show !== false });
  }
  for (const s of BUILTIN_SECTIONS) if (!seen.has(s.id)) out.push({ id: s.id, show: true });
  return out;
}
