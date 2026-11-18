import { describe, expect, it } from "vitest";
import { chooseLevel, effectiveLevel, migrateLevel } from "./levelPrefs";

describe("阅读门槛", () => {
  it("把旧的手动档位迁过来并关闭跟随", () => {
    expect(migrateLevel({ level: 0, follow: true }, 8000)).toEqual({ level: 8000, follow: false });
  });
  it("没有旧值时不迁移", () => {
    expect(migrateLevel({ level: 0, follow: true }, null)).toBeNull();
  });
  it("旧值无效时不迁移", () => {
    expect(migrateLevel({ level: 0, follow: true }, "abc")).toBeNull();
    expect(migrateLevel({ level: 0, follow: true }, -5)).toBeNull();
    expect(migrateLevel({ level: 0, follow: true }, 99999)).toBeNull();
  });
  it("已经设置过就不再迁移", () => {
    expect(migrateLevel({ level: 7800, follow: true }, 8000)).toBeNull();
  });
  it("未设置时用默认门槛", () => {
    expect(effectiveLevel(0)).toBe(5000);
    expect(effectiveLevel(7800)).toBe(7800);
  });
  it("选「我的词汇量」时跟随测试结果", () => {
    expect(chooseLevel("me", 7800)).toEqual({ level: 7800, follow: true });
  });
  it("选固定档位或自定义数字时不再跟随", () => {
    expect(chooseLevel(8000, 7800)).toEqual({ level: 8000, follow: false });
    expect(chooseLevel(6543, 0)).toEqual({ level: 6543, follow: false });
  });
  it("自定义数字限制在 1 到 40000", () => {
    expect(chooseLevel(99999, 0)).toEqual({ level: 40000, follow: false });
    expect(chooseLevel(0, 0)).toEqual({ level: 1, follow: false });
  });
});
