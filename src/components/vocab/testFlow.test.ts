import { describe, expect, it } from "vitest";
import { keyToAction, withAnswer, withCheck } from "./testFlow";

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
