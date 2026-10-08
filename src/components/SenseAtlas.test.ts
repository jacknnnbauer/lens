import { describe, expect, it } from "vitest";
import { dictBadge } from "./SenseAtlas";

describe("dictBadge", () => {
  it("abbreviates dictionary names", () => {
    expect(dictBadge("牛津高阶英汉双解词典 第 8 版")).toBe("牛");
    expect(dictBadge("Merriam-Webster's Collegiate Dictionary")).toBe("MW");
    expect(dictBadge("Collins Thesaurus")).toBe("CT");
    expect(dictBadge("COCA2W")).toBe("C");
  });
});
