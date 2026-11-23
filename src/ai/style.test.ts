import { describe, expect, it } from "vitest";
import { CLAUSES, styleKey } from "./style";

describe("例句样式", () => {
  it("default_style_key_is_empty_like_rust", () => {
    expect(styleKey(undefined)).toBe("");
    expect(styleKey({ length: "medium", clauses: [] })).toBe("");
    expect(styleKey({ length: "huge", clauses: ["bogus"] })).toBe("");
  });

  it("clauses_follow_list_order_without_duplicates", () => {
    expect(styleKey({ length: "short", clauses: ["time", "subject", "time", "x"] })).toBe("short|subject,time");
    expect(styleKey({ length: "long", clauses: [] })).toBe("long|");
    expect(CLAUSES).toHaveLength(15);
  });
});
