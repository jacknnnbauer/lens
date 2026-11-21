import { describe, expect, it } from "vitest";
import type { WeeklyReport } from "../ipc/weekly";
import { formatDuration, layoutCard, wrapText } from "./layout";

// 假测量：每个字符 10px
const measure = (s: string) => s.length * 10;

const empty: WeeklyReport = { days: [], readSeconds: 0, readWords: 0, vocab: null, newWords: 0, newWordSamples: [], quotes: [] };
const full: WeeklyReport = {
  days: [
    { day: "2026-11-16", seconds: 3600 },
    { day: "2026-11-20", seconds: 8400 },
  ],
  readSeconds: 12000,
  readWords: 21000,
  vocab: { value: 7960, previous: 7820, points: [["2026-11-17", 7900], ["2026-11-21", 7960]] },
  newWords: 36,
  newWordSamples: ["ubiquitous", "reverie"],
  quotes: [{ quote: "I am no bird.", note: "好句", title: "Jane Eyre", chapter: "Chapter XXIII", createdAt: 0 }],
};

describe("卡片排版", () => {
  it("empty_report_has_no_card", () => {
    expect(layoutCard(empty, 0, measure, "2026-11-16")).toBeNull();
    // 读了不到一分钟也不算
    expect(layoutCard({ ...empty, readSeconds: 40 }, 0, measure, "2026-11-16")).toBeNull();
  });

  it("missing_blocks_are_skipped_and_rest_move_up", () => {
    const all = layoutCard(full, 0, measure, "2026-11-16")!;
    const onlyWords = layoutCard({ ...empty, newWords: 3, newWordSamples: ["a"] }, 0, measure, "2026-11-16")!;
    expect(all.time && all.vocab && all.words && all.quote).toBeTruthy();
    expect(onlyWords.time ?? onlyWords.vocab ?? onlyWords.quote).toBeUndefined();
    // 没有第一行（时长、词汇量）时，生词块顶到原来第一行的位置
    expect(onlyWords.words!.top).toBe(all.time!.top);
  });

  it("time_block_has_seven_days_and_word_estimate", () => {
    const t = layoutCard(full, 0, measure, "2026-11-16")!.time!;
    expect(t.daySeconds).toEqual([3600, 0, 0, 0, 8400, 0, 0]);
    expect(t.text).toBe("3 小时 20 分");
    expect(t.sub).toBe("读了 2 天 · 约 2.1 万词");
  });

  it("long_quote_is_cut_to_6_lines_with_ellipsis", () => {
    const long = { ...full, quotes: [{ ...full.quotes[0], quote: "word ".repeat(400), note: "注".repeat(500) }] };
    const q = layoutCard(long, 0, measure, "2026-11-16")!.quote!;
    expect(q.lines.length).toBe(6);
    expect(q.lines[5].endsWith("…")).toBe(true);
    expect(q.noteLines.length).toBe(3);
  });

  it("quote_index_cycles", () => {
    const two = { ...full, quotes: [full.quotes[0], { ...full.quotes[0], quote: "second" }] };
    expect(layoutCard(two, 3, measure, "2026-11-16")!.quote!.lines[0]).toContain("second");
  });
});

describe("换行", () => {
  it("wrap_keeps_english_words_whole", () => {
    expect(wrapText("hello brave new world", 120, measure, 5)).toEqual(["hello brave", "new world"]);
  });

  it("chinese_wraps_by_character", () => {
    expect(wrapText("一二三四五六七", 30, measure, 5)).toEqual(["一二三", "四五六", "七"]);
  });

  it("too_many_lines_end_with_ellipsis", () => {
    const lines = wrapText("一二三四五六七八九", 30, measure, 2);
    expect(lines).toEqual(["一二三", "四五…"]);
  });
});

describe("时长", () => {
  it("formats_hours_and_minutes", () => {
    expect(formatDuration(12000)).toBe("3 小时 20 分");
    expect(formatDuration(2700)).toBe("45 分钟");
    expect(formatDuration(7200)).toBe("2 小时");
  });
});
