import { describe, expect, it } from "vitest";
import { chapterFor, pageFor } from "./viewMap";

// 三章，分别从第 1、10、30 页开始；共 50 页
const PAGES = [1, 10, 30];

describe("pageFor", () => {
  it("maps a chapter position to a page", () => {
    expect(pageFor(PAGES, 1, 0.5, 50)).toBe(20);
    expect(pageFor(PAGES, 0, 0, 50)).toBe(1);
  });
  it("clamps to the last page", () => {
    expect(pageFor(PAGES, 2, 1, 50)).toBe(50);
  });
  it("survives a bad chapter index", () => {
    expect(pageFor(PAGES, 9, 0.5, 50)).toBe(50);
    expect(pageFor([], 0, 0.5, 50)).toBe(1);
  });
});

describe("chapterFor", () => {
  it("maps a page to a chapter position", () => {
    expect(chapterFor(PAGES, 20, 50)).toEqual({ chapter: 1, pos: 0.5 });
    expect(chapterFor(PAGES, 1, 50)).toEqual({ chapter: 0, pos: 0 });
  });
  it("pages before the first chapter belong to it", () => {
    expect(chapterFor([3, 8], 1, 20)).toEqual({ chapter: 0, pos: 0 });
  });
  it("round-trips roughly", () => {
    const back = chapterFor(PAGES, pageFor(PAGES, 2, 0.4, 50), 50);
    expect(back.chapter).toBe(2);
    expect(Math.abs(back.pos - 0.4)).toBeLessThan(0.06);
  });
});
