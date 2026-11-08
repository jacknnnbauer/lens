import { describe, expect, it } from "vitest";
import type { Digest } from "../ipc/digest";
import { buildDigestSections, chapterLabel } from "./digest";
import { buildStudyBody } from "./study";

const empty = {
  oneLine: "", paragraph: "", talk: "", structure: [], characters: [], relations: "", background: "", glossary: [], timeline: [], motifs: [], themes: [],
  technique: "", reception: "", criticism: [], origins: "", misconceptions: [], topics: [], golden: [], further: [], faq: [], quiz: [],
};

const digest: Digest = {
  model: "deepseek-v4-flash",
  dropped: 2,
  chapters: [
    { idx: 3, title: "CHAPTER 1. Loomings.", summary: "以实玛利决定出海。", kind: "plot", events: ["决定出海"], characters: ["Ishmael"], quotes: [1] },
    { idx: 34, title: "CHAPTER 32. Cetology.", summary: "鲸类学。", kind: "digression", events: [], characters: [], quotes: [] },
  ],
  quotes: [{ id: 1, chapter: 3, text: "Call me Ishmael.", zh: "叫我以实玛利。", why: "全书第一句 <名句>" }],
  synthesis: {
    ...empty,
    oneLine: "一个船长追杀白鲸。",
    structure: [{ title: "岸上", from: 3, to: 34, summary: "上船前" }],
    characters: [{ name: "以实玛利（Ishmael）", role: "叙述者", arc: "", scenes: "", meaning: "", quote: 1 }],
    golden: [1],
    quiz: [{ q: "叙述者叫什么？", a: "以实玛利", chapter: 3 }],
  },
};

describe("digest sections", () => {
  it("labels chapters by number when the title has one", () => {
    expect(chapterLabel("CHAPTER 36. The Quarter-Deck.")).toBe("第 36 章");
    expect(chapterLabel("Epilogue")).toBe("Epilogue");
  });

  it("renders the parts in order, with quotes, chapter bookmarks and quiz answers after the questions", () => {
    const html = buildDigestSections(digest);
    const order = ["整体", "人物与世界", "逐章提要", "细节自测"].map((h) => html.indexOf(`<h2>${h}`));
    expect(order.every((i, k) => i >= 0 && (k === 0 || i > order[k - 1]))).toBe(true);
    expect(html).toContain('<p class="dg-en">Call me Ishmael.</p>');
    expect(html).toContain("&lt;名句&gt;");
    expect(html).toContain('<h3>CHAPTER 1. Loomings.</h3>');
    expect(html).toContain('<div class="dg-part">岸上</div>');
    expect(html).toContain('data-kind="digression"');
    expect(html.indexOf("叙述者叫什么")).toBeLessThan(html.indexOf("<h3>答案</h3>"));
    expect(html).toContain("2 段模型给出的引文");
    // 没内容的部分不出现
    expect(html).not.toContain("<h2>评价与影响");
  });

  it("goes first in the study pack and changes the kicker", () => {
    const body = buildStudyBody({
      title: "Moby-Dick",
      author: null,
      words: [],
      chapters: [],
      order: "chapter",
      style: "paperWhite",
      date: new Date(2026, 10, 8),
      level: 8000,
      options: { sentence: true, definition: true, examples: false, phonetics: true, roadmap: false, index: false, simple: true, detail: true },
      stats: { total: 1, minutes: 1, coverage: 1 },
      guides: [],
      digest,
    });
    expect(body).toContain('<div class="ar-kicker">精读本</div>');
    expect(body.indexOf("dg-whole")).toBeGreaterThan(body.indexOf("</header>"));
  });
});
