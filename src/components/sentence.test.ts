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

  it("中文：词和成语按词查，整句当句子", () => {
    for (const q of ["精品店", "画蛇添足", "自由主义"]) expect(isSentence(q)).toBe(false);
    for (const q of ["这里洋溢着自由的气息。", "旁边有上好的咖啡和时装精品店", "他说：不行"]) expect(isSentence(q)).toBe(true);
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

describe("splitWords 中文", () => {
  it("连续汉字分成词，跳过标点，区间对得上", () => {
    const s = "上好的咖啡、时装精品店。";
    const ws = splitWords(s);
    for (const w of ws) expect(s.slice(w.start, w.end)).toBe(w.text);
    expect(ws.map((w) => w.text)).toContain("咖啡");
    expect(ws.some((w) => /[、。]/.test(w.text))).toBe(false);
  });

  it("中英混排各切各的", () => {
    expect(splitWords("这家boutique很贵").map((w) => w.text)).toContain("boutique");
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

  it("中文：光标在两个词之间取后一个，句末取前一个", () => {
    const z = "美味的沙拉";
    const ws = splitWords(z);
    const sala = ws.find((w) => w.text === "沙拉");
    expect(sala).toBeTruthy();
    expect(z.slice(...Object.values(wordSpan(z, sala!.start, sala!.start)!))).toBe("沙拉");
    expect(z.slice(...Object.values(wordSpan(z, z.length, z.length)!))).toBe("沙拉");
  });
});
