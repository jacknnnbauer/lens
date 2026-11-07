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
  options: { sentence: true, definition: false, examples: false, phonetics: true, roadmap: true, index: true, simple: true, detail: true },
  stats: { total: 3000, minutes: 17, coverage: 0.981 },
  guides: [],
});

describe("study export", () => {
  it("groups by chapter, numbers chapters that have text, adds a roadmap and an index", () => {
    const html = buildStudyBody(input("chapter"));
    expect(html).toContain("Herman Melville");
    expect(html).toContain("阅读路线表");
    // 分组标题用书自己的章节标题
    expect(html).toMatch(/<h3>Loomings<small aria-hidden="true">/);
    expect(html.indexOf("<h3>Loomings")).toBeLessThan(html.indexOf("<h3>The Carpet-Bag"));
    expect(html).toContain("全书 4 次");
    expect(html).toContain("<mark>whale</mark>");
    // 简明单词表在详细词表前面，两份编号一致
    expect(html.indexOf("简明单词表")).toBeLessThan(html.indexOf("详细词表"));
    expect(html).toContain('<td class="sl-w">whale</td>');
    expect(html).toContain("字母索引");
  });

  it("alphabetical lists show the chapter and skip the index", () => {
    const html = buildStudyBody(input("alpha"));
    expect(html).toMatch(/<h3>H<small aria-hidden="true">/);
    expect(html).toContain("首见于 The Carpet-Bag");
    expect(html).not.toContain("字母索引");
  });

  it("lists every Chinese sense when there are several", () => {
    const i = input("chapter");
    i.words[0] = { ...i.words[0], gloss: "十分清楚", senses: ["十分清楚；非常明白", "大量地；丰盛地"] };
    const html = buildStudyBody(i);
    expect(html).toContain('<span class="gl-sense"><b>1</b>十分清楚；非常明白</span><span class="gl-sense"><b>2</b>大量地；丰盛地</span>');
  });

  it("can make only the simple list, or add a Wikipedia guide with sources", () => {
    const i = input("chapter");
    i.options = { ...i.options, detail: false };
    i.guides = [{
      book: { lang: "zh", title: "白鲸", url: "https://zh.wikipedia.org/wiki/x", sections: [{ kind: "lead", heading: "", text: "第一段\n第二段" }, { kind: "themes", heading: "评述", text: "主题" }] },
      author: { lang: "zh", title: "赫尔曼·梅尔维尔", url: "https://zh.wikipedia.org/wiki/y", sections: [{ kind: "lead", heading: "", text: "美国小说家" }] },
      plotHidden: true,
      grokipedia: "https://grokipedia.com/page/Moby-Dick",
      ai: null,
    }];
    const html = buildStudyBody(i);
    expect(html).not.toContain("详细词表");
    expect(html).not.toContain("字母索引");
    expect(html).toContain("<h2>读前导读</h2>");
    expect(html).toContain("<p>第一段</p><p>第二段</p>");
    expect(html).toContain("主题与风格 · 评述");
    expect(html).toContain("作者 · 赫尔曼·梅尔维尔");
    expect(html).toContain("CC BY-SA 4.0");
    expect(html).toContain("不剧透");
    expect(html).toContain("https://grokipedia.com/page/Moby-Dick");
    // AI 写的：注明模型、可能有错，不写 CC BY-SA 和 Grokipedia
    i.guides = [{ ...i.guides[0], book: { ...i.guides[0].book, url: "" }, grokipedia: "", ai: { model: "deepseek-flash", known: false } }];
    const ai = buildStudyBody(i);
    expect(ai).toContain("读前导读 · AI 生成");
    expect(ai).toContain("deepseek-flash");
    expect(ai).toContain("不了解这本书");
    expect(ai).not.toContain("CC BY-SA");
    expect(ai).not.toContain("grokipedia.com");
  });

  it("formats durations", () => {
    expect(duration(45)).toBe("45 分钟");
    expect(duration(120)).toBe("2 小时");
    expect(duration(200)).toBe("3 小时 20 分");
  });
});
