import { describe, expect, it } from "vitest";
import { en } from "../../i18n/en";
import { LEARNER_MEDIAN, learnerCurve, learnerPercentile, levelInfo, MILESTONES, percentileText, scalePos } from "./levelInfo";

describe("levelInfo", () => {
  it("刚好到参照点就算这一档", () => {
    expect(levelInfo(6000).title).toContain("六级");
    expect(levelInfo(5999).title).toContain("四级");
  });
  it("很少的词汇量算入门", () => {
    expect(levelInfo(0).title).toBe("入门阶段");
  });
  it("测量上限算接近母语者", () => {
    expect(levelInfo(40000).title).toBe("接近英语母语者");
  });
  it("每一档都有评价、能读什么和下一步", () => {
    for (const v of [0, 2000, 3500, 5000, 7000, 9000, 15000, 30000]) {
      const i = levelInfo(v);
      expect(i.summary && i.canRead && i.next).toBeTruthy();
    }
  });
});

describe("scalePos", () => {
  it("两端夹在 0 和 1", () => {
    expect(scalePos(10)).toBe(0);
    expect(scalePos(99999)).toBe(1);
  });
  it("参照点从左到右排", () => {
    const xs = MILESTONES.map((m) => scalePos(m.at));
    expect([...xs].sort((a, b) => a - b)).toEqual(xs);
  });
});

describe("英文界面", () => {
  it("每一档的文字和刻度名都有英文", () => {
    const texts = MILESTONES.flatMap((m) => [m.short, m.label]);
    for (const v of [0, 1600, 3000, 4500, 6000, 8000, 13000, 20000]) {
      const i = levelInfo(v);
      texts.push(i.title, i.summary, i.canRead, i.next);
    }
    expect(texts.filter((x) => !(x in en))).toEqual([]);
  });
});

describe("刻度标签", () => {
  it("相邻参照点在 560px 宽的刻度上至少隔开 40px，单行排得下", () => {
    const xs = MILESTONES.map((m) => scalePos(m.at) * 560);
    for (let i = 1; i < xs.length; i++) expect(xs[i] - xs[i - 1]).toBeGreaterThan(40);
  });
});

describe("学习者参照曲线", () => {
  it("中位数正好超过一半", () => {
    expect(learnerPercentile(LEARNER_MEDIAN)).toBeCloseTo(50, 3);
  });
  it("词汇量越大超过的人越多", () => {
    expect(learnerPercentile(8000)).toBeGreaterThan(learnerPercentile(6000));
    expect(learnerPercentile(0)).toBe(0);
  });
  it("显示的百分比收在 1–99", () => {
    expect(percentileText(50)).toBe(1);
    expect(percentileText(40000)).toBe(99);
  });
  it("曲线最高点是 1，两头接近 0", () => {
    const c = learnerCurve();
    expect(Math.max(...c.map((p) => p[1]))).toBe(1);
    expect(c[c.length - 1][1]).toBeLessThan(0.1);
  });
});
