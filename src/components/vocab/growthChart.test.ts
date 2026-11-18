import { describe, expect, it } from "vitest";
import { buildChart, daysBetween } from "./growthChart";

const test = (day: string, value: number) => ({ day, takenAt: 0, value, low: value - 100, high: value + 100 });

describe("成长曲线", () => {
  it("没有快照的日子沿用前一天的数字", () => {
    const c = buildChart(
      [
        { day: "2026-01-01", value: 5000 },
        { day: "2026-01-04", value: 5030 },
      ],
      [test("2026-01-01", 5000)],
      "all",
      "2026-01-05",
    );
    expect(c.points.map((p) => p.y)).toEqual([5000, 5000, 5000, 5030, 5030]);
    expect(c.points.map((p) => p.x)).toEqual([0, 1, 2, 3, 4]);
  });

  it("只取所选范围内的日子，范围开始前的数字接着用", () => {
    const c = buildChart(
      [
        { day: "2026-01-01", value: 5000 },
        { day: "2026-03-30", value: 5400 },
      ],
      [test("2026-01-01", 5000)],
      30,
      "2026-03-31",
    );
    expect(c.points).toHaveLength(30);
    expect(c.points[0].y).toBe(5000);
    expect(c.points[29].y).toBe(5400);
    expect(c.dots).toHaveLength(0);
  });

  it("每次测试都有一个圆点", () => {
    const c = buildChart(
      [
        { day: "2026-01-01", value: 5000 },
        { day: "2026-01-03", value: 5600 },
      ],
      [test("2026-01-01", 5000), test("2026-01-03", 5600)],
      "all",
      "2026-01-03",
    );
    expect(c.dots).toEqual([
      { x: 0, y: 5000, value: 5000 },
      { x: 2, y: 5600, value: 5600 },
    ]);
  });

  it("只有一天数据时也能画，上下限不相等", () => {
    const c = buildChart([{ day: "2026-01-01", value: 5000 }], [test("2026-01-01", 5000)], 90, "2026-01-01");
    expect(c.points).toHaveLength(1);
    expect(c.min).toBeLessThan(c.max);
  });

  it("算两个日期相隔的天数", () => {
    expect(daysBetween("2026-01-31", "2026-03-01")).toBe(29);
  });
});
