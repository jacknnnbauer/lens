import { describe, expect, it } from "vitest";
import type { ChapterRow } from "../ipc/study";
import { buildStudyBody, duration, type StudyExportInput, type StudyExportWord } from "./study";

const word = (lemma: string, chapter: number, count = 1): StudyExportWord => ({
  lemma,
  surface: lemma,
  status: "new",
  pos: "n.",
  gloss: "释义",
  sentence: `The ${lemma} rose.`,
  hlStart: 4,
  hlEnd: 4 + lemma.length,
  entry: null,
  count,
  chapter,
});

const chapters: ChapterRow[] = [
  { idx: 3, title: "Loomings", tokens: 2000, minutes: 12, firstWords: 2, newTokens: 5, density: 2.5 },
  { idx: 4, title: "The Carpet-Bag", tokens: 1000, minutes: 6, firstWords: 1, newTokens: 4, density: 4 },
];

const input = (order: "chapter" | "alpha"): StudyExportInput => ({
  title: "Moby-Dick",
  author: "Herman Melville",
  words: order === "chapter" ? [word("whale", 3, 4), word("leviathan", 3), word("harpoon", 4)] : [word("harpoon", 4), word("leviathan", 3), word("whale", 3, 4)],
  chapters,
  order,
  style: "paperWhite",
  date: new Date(2026, 10, 7),
  level: 8000,
  options: { sentence: true, definition: false, examples: false, phonetics: true, roadmap: true, index: true },
  stats: { total: 3000, minutes: 17, coverage: 0.981 },
});

describe("study export", () => {
  it("groups by chapter, numbers chapters that have text, adds a roadmap and an index", () => {
    const html = buildStudyBody(input("chapter"));
    expect(html).toContain("Herman Melville");
    expect(html).toContain("阅读路线表");
    // 分组标题用书自己的章节标题
    expect(html).toMatch(/<h2>Loomings<small aria-hidden="true">/);
    expect(html.indexOf("<h2>Loomings")).toBeLessThan(html.indexOf("<h2>The Carpet-Bag"));
    expect(html).toContain("全书 4 次");
    expect(html).toContain("<mark>whale</mark>");
    // 只有第一组另起一页
    expect(html.match(/data-follow/g)?.length).toBe(1);
    expect(html).toContain("字母索引");
  });

  it("alphabetical lists show the chapter and skip the index", () => {
    const html = buildStudyBody(input("alpha"));
    expect(html).toMatch(/<h2>H<small aria-hidden="true">/);
    expect(html).toContain("首见于 The Carpet-Bag");
    expect(html).not.toContain("字母索引");
  });

  it("lists every Chinese sense when there are several", () => {
    const i = input("chapter");
    i.words[0] = { ...i.words[0], gloss: "十分清楚", senses: ["十分清楚；非常明白", "大量地；丰盛地"] };
    const html = buildStudyBody(i);
    expect(html).toContain('<span class="gl-sense"><b>1</b>十分清楚；非常明白</span><span class="gl-sense"><b>2</b>大量地；丰盛地</span>');
  });

  it("formats durations", () => {
    expect(duration(45)).toBe("45 分钟");
    expect(duration(120)).toBe("2 小时");
    expect(duration(200)).toBe("3 小时 20 分");
  });
});
