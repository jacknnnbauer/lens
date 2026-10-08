import { describe, expect, it } from "vitest";
import { formatInterval } from "./ReviewSession";

describe("formatInterval", () => {
  it("picks a readable unit", () => {
    expect(formatInterval(10 * 60_000)).toBe("10 分钟");
    expect(formatInterval(3 * 3600_000)).toBe("3 小时");
    expect(formatInterval(4 * 86400_000)).toBe("4 天");
    expect(formatInterval(90 * 86400_000)).toBe("3 个月");
    expect(formatInterval(548 * 86400_000)).toBe("1.5 年");
  });
});
