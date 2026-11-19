import { describe, expect, it } from "vitest";
import { en } from "../../i18n/en";
import { levelInfo, MILESTONES, scalePos } from "./levelInfo";

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
    const texts = [...MILESTONES.map((m) => m.label)];
    for (const v of [0, 1600, 3000, 4500, 6000, 8000, 13000, 20000]) {
      const i = levelInfo(v);
      texts.push(i.title, i.summary, i.canRead, i.next);
    }
    expect(texts.filter((x) => !(x in en))).toEqual([]);
  });
});
