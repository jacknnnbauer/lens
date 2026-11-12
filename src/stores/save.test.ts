import { describe, expect, it } from "vitest";
import type { Card } from "./lookup";
import { briefOf, canSave } from "./save";

const card = (summary: string, status: Card["status"] = "hit", name = "D"): Card => ({
  id: "d",
  name,
  source: "local",
  status,
  error: null,
  ms: 1,
  cachedAt: null,
  stale: false,
  webUrl: null,
  entries: [{ headword: "x", html: "", summary }],
});

describe("briefOf", () => {
  it("strips a repeated headword and leading punctuation", () => {
    expect(briefOf([card("ubiquitous · adj. 无处不在的")], "Ubiquitous")).toBe("adj. 无处不在的");
  });
  it("uses the first hit and truncates", () => {
    const long = "a".repeat(120) + "…";
    expect(briefOf([card("miss", "miss"), card(long)], "w")).toBe(`${"a".repeat(80)}…`);
  });
  it("skips frequency / collocation dictionaries and number soup", () => {
    const coca = card("v 11677 1278150678 27115425 n 1572573315421119314728", "hit", "COCA Frequency 60000");
    expect(briefOf([coca, card("whack · v. 猛击")], "whack")).toBe("v. 猛击");
    expect(briefOf([card("v 11677 1278150678 27115425"), card("to hit hard")], "whack")).toBe("to hit hard");
    expect(briefOf([coca], "whack")).toBeNull();
  });
  it("returns null without hits", () => {
    expect(briefOf([card("x", "miss")], "w")).toBeNull();
  });
});

describe("canSave", () => {
  it("allows saving a word no dictionary has once the lookup is done, if the setting allows", () => {
    expect(canSave([card("a"), card("", "miss")], false)).toBe(true);
    expect(canSave([card("", "miss")], false)).toBe(false);
    expect(canSave([card("", "miss")], true)).toBe(true);
    // 还在查：等查完再说，免得先收了一个没有简释的
    expect(canSave([card("", "miss"), card("", "pending")], true)).toBe(false);
  });
});
