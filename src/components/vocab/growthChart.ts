// 成长曲线的数据整理：按天补齐、按范围截取。

import type { VocabDayPoint, VocabTestPoint } from "../../ipc/vocabTest";

export type ChartRange = 30 | 90 | "all";

export interface Chart {
  /** x = 从范围第一天算起的第几天，y = 当天的词汇量 */
  points: { x: number; y: number }[];
  /** 测试当天的圆点 */
  dots: { x: number; y: number; value: number }[];
  min: number;
  max: number;
}

const DAY_MS = 86_400_000;
const toDays = (day: string) => Date.UTC(Number(day.slice(0, 4)), Number(day.slice(5, 7)) - 1, Number(day.slice(8, 10))) / DAY_MS;

export function daysBetween(from: string, to: string): number {
  return toDays(to) - toDays(from);
}

export function buildChart(days: VocabDayPoint[], tests: VocabTestPoint[], range: ChartRange, today: string): Chart {
  const end = toDays(today);
  const first = days.length ? toDays(days[0].day) : end;
  const start = range === "all" ? first : Math.max(first, end - range + 1);
  const known = new Map(days.map((d) => [toDays(d.day), d.value]));
  const points: Chart["points"] = [];
  let value = days[0]?.value ?? 0;
  // 范围开始之前最后一天的数字接着用
  for (const d of days) if (toDays(d.day) <= start) value = d.value;
  for (let day = start; day <= end; day++) {
    value = known.get(day) ?? value;
    points.push({ x: day - start, y: value });
  }
  const dots = tests
    .filter((t) => toDays(t.day) >= start && toDays(t.day) <= end)
    .map((t) => ({ x: toDays(t.day) - start, y: points[toDays(t.day) - start].y, value: t.value }));
  const ys = points.map((p) => p.y);
  const low = Math.min(...ys);
  const high = Math.max(...ys);
  const pad = Math.max(50, (high - low) * 0.15);
  return { points, dots, min: Math.max(0, low - pad), max: high + pad };
}
