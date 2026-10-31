import { describe, expect, it } from "vitest";
import { isSentence, splitWords, wordSpan } from "./sentence";

describe("isSentence", () => {
  it("把词和词组当词条", () => {
    for (const q of ["deplete", "look forward to", "on the other hand", "take off.", "  a priori "]) expect(isSentence(q)).toBe(false);
  });

  it("把整句当句子", () => {
    for (const q of ["The company addressed the issue.", "Why did it fail?", "Resources were depleted over the winter months", "He said “no.”"])
      expect(isSentence(q)).toBe(true);
  });
});

describe("splitWords", () => {
  it("带区间切词，撇号连字符算在词里", () => {
    const s = "It's a well-known fact.";
    expect(splitWords(s).map((w) => [w.text, s.slice(w.start, w.end)])).toEqual([
      ["It's", "It's"],
      ["a", "a"],
      ["well-known", "well-known"],
      ["fact", "fact"],
    ]);
  });
});

describe("wordSpan", () => {
  const s = "Resources were rapidly depleted.";
  const text = (r: { start: number; end: number } | null) => (r ? s.slice(r.start, r.end) : null);

  it("光标在词里、词边取这个词", () => {
    expect(text(wordSpan(s, 26, 26))).toBe("depleted");
    expect(text(wordSpan(s, 23, 23))).toBe("depleted");
    expect(text(wordSpan(s, 31, 31))).toBe("depleted");
  });

  it("选了一段扩到整词，反向选也行", () => {
    expect(text(wordSpan(s, 17, 25))).toBe("rapidly depleted");
    expect(text(wordSpan(s, 25, 17))).toBe("rapidly depleted");
  });

  it("只选到标点时没有词", () => {
    expect(wordSpan(s, 31, 32)).toBeNull();
  });
});
