import { describe, expect, it } from "vitest";
import { formatEstimate, keyToAction, withAnswer, withCheck } from "./testFlow";

describe("词汇量测试的操作", () => {
  it("键盘：左不认识、右认识", () => {
    expect(keyToAction("ArrowLeft", "word")).toEqual({ known: false });
    expect(keyToAction("ArrowRight", "word")).toEqual({ known: true });
  });
  it("抽查：1–3 选释义，4 是不确定", () => {
    expect(keyToAction("2", "check")).toEqual({ option: 1 });
    expect(keyToAction("4", "check")).toEqual({ option: null });
  });
  it("词题不响应数字键，抽查题不响应方向键", () => {
    expect(keyToAction("1", "word")).toBeNull();
    expect(keyToAction("ArrowLeft", "check")).toBeNull();
  });
  it("答一道词题就追加一条记录", () => {
    const a = withAnswer([], { word: "house", band: 0 }, true);
    expect(a).toEqual([{ word: "house", band: 0, saidKnown: true, check: null }]);
  });
  it("抽查结果写回最后一道词题，选「不确定」算答错", () => {
    const a = withAnswer([], { word: "house", band: 0 }, true);
    expect(withCheck(a, 1, 1)[0].check).toBe(true);
    expect(withCheck(a, 0, 1)[0].check).toBe(false);
    expect(withCheck(a, null, 1)[0].check).toBe(false);
  });
});

describe("结果显示", () => {
  it("按百位显示结果和区间", () => {
    expect(formatEstimate({ value: 7812, low: 7420, high: 8190, capped: false, optimistic: false })).toBe("约 7,800（7,400–8,200）");
  });
  it("超过上限显示 40,000+", () => {
    expect(formatEstimate({ value: 40000, low: 36000, high: 40000, capped: true, optimistic: false })).toBe("40,000+");
  });
  it("一个词都不认识时显示 0", () => {
    expect(formatEstimate({ value: 0, low: 0, high: 0, capped: false, optimistic: false })).toBe("约 0（0–0）");
  });
});
