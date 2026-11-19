import { describe, expect, it } from "vitest";
import { locate, makeAnchor } from "./anchor";

const TEXT = "It was the best of times, it was the worst of times, it was the age of wisdom, it was the age of foolishness.";

describe("makeAnchor", () => {
  it("takes up to 32 characters around the quote", () => {
    const s = TEXT.indexOf("the worst");
    const a = makeAnchor(TEXT, s, s + "the worst of times".length);
    expect(a.quote).toBe("the worst of times");
    expect(a.prefix).toBe(TEXT.slice(s - 32, s));
    expect(a.suffix).toBe(TEXT.slice(s + 18, s + 50));
  });

  it("anchor_at_text_edges", () => {
    const a = makeAnchor(TEXT, 0, 6);
    expect(a.prefix).toBe("");
    expect(locate(TEXT, a)).toEqual([0, 6]);
    const e = makeAnchor(TEXT, TEXT.length - 12, TEXT.length);
    expect(e.suffix).toBe("");
    expect(locate(TEXT, e)).toEqual([TEXT.length - 12, TEXT.length]);
  });
});

describe("locate", () => {
  it("finds_exact", () => {
    const s = TEXT.indexOf("age of wisdom");
    expect(locate(TEXT, makeAnchor(TEXT, s, s + 13))).toEqual([s, s + 13]);
  });

  it("survives_text_inserted_before", () => {
    const s = TEXT.indexOf("age of wisdom");
    const a = makeAnchor(TEXT, s, s + 13);
    const edited = "x".repeat(200) + " A new opening line. " + TEXT;
    const shift = edited.length - TEXT.length;
    expect(locate(edited, a)).toEqual([s + shift, s + shift + 13]);
  });

  it("picks_nearest_duplicate", () => {
    // 「it was」出现多次：按前后文挑出第三处
    const hits = [...TEXT.matchAll(/it was/g)].map((m) => m.index!);
    const a = makeAnchor(TEXT, hits[2], hits[2] + 6);
    expect(locate(TEXT, a)).toEqual([hits[2], hits[2] + 6]);
    // 前后文改了一点也还是这一处
    const edited = TEXT.replace("age of wisdom", "age of WISDOM");
    expect(locate(edited, a)).toEqual([hits[2], hits[2] + 6]);
  });

  it("returns_null_when_quote_changed", () => {
    const s = TEXT.indexOf("foolishness");
    const a = makeAnchor(TEXT, s, s + 11);
    expect(locate(TEXT.replace("foolishness", "folly"), a)).toBeNull();
  });
});
