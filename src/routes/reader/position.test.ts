import { describe, expect, it } from "vitest";
import { readPos, spanAt } from "./position";

/** 假的词 span：起点、屏幕上的上下边 */
function span(start: number, top: number): HTMLElement {
  return { dataset: { start: String(start) }, getBoundingClientRect: () => ({ top, bottom: top + 20 }) } as unknown as HTMLElement;
}
// 10 个词，每个 10 字符、每行一个（行高 20），正文共 100 字符；屏幕顶在 y=100
const spans = Array.from({ length: 10 }, (_, i) => span(i * 10, i * 20));

describe("reading position by text", () => {
  it("read_pos_is_first_visible_word_over_text_length", () => {
    // y=100 处：第 5 个词（top 100、bottom 120）是第一个露出来的
    expect(readPos(spans, 100, 100)).toBe(0.5);
    expect(readPos(spans, 100, -50)).toBe(0);
  });

  it("read_pos_past_the_end_is_one", () => {
    expect(readPos(spans, 100, 10_000)).toBe(1);
  });

  it("span_at_finds_first_word_at_or_after_fraction", () => {
    expect(spanAt(spans, 0.5, 100)?.dataset.start).toBe("50");
    expect(spanAt(spans, 0.47, 100)?.dataset.start).toBe("50");
    expect(spanAt(spans, 0, 100)?.dataset.start).toBe("0");
    expect(spanAt(spans, 1, 100)?.dataset.start).toBe("90");
  });

  it("empty_chapter_has_no_span", () => {
    expect(spanAt([], 0.3, 0)).toBeNull();
    expect(readPos([], 0, 0)).toBe(0);
  });

  it("round_trip_lands_on_the_same_word", () => {
    const pos = readPos(spans, 100, 130);
    expect(spanAt(spans, pos, 100)).toBe(spans[6]);
  });
});
