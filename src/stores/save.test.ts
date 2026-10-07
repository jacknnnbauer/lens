import { describe, expect, it } from "vitest";
import type { Card } from "./lookup";
import { briefOf } from "./save";

const card = (summary: string, status: Card["status"] = "hit"): Card => ({
  id: "d",
  name: "D",
  status,
  error: null,
  ms: 1,
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
  it("returns null without hits", () => {
    expect(briefOf([card("x", "miss")], "w")).toBeNull();
  });
});
