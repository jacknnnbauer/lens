// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { buildArticleBody, markArticle, type ArticleExportInput } from "./article";

const w = (lemma: string, start: number) => `<span class="lens-w" data-start="${start}" data-lemma="${lemma}" data-status="new">${lemma}</span>`;
const HTML = `<h1>${w("bike", 0)}</h1><p>An ${w("old", 3)} ${w("bike", 7)}, ${w("rusty", 12)}.</p><p>The ${w("rusty", 30)} one.</p>`;

describe("markArticle", () => {
  it("marks chosen words, numbers the first occurrence, unwraps the rest", () => {
    const r = markArticle(HTML, new Map([["rusty", 1], ["bike", 2]]));
    expect(r.html).not.toContain("lens-w");
    expect(r.html).not.toContain("data-");
    expect(r.html.match(/class="aw"/g)?.length).toBe(4);
    expect(r.html.match(/<sup class="an">1<\/sup>/g)?.length).toBe(1);
    expect(r.blocks.map((b) => b.first)).toEqual([[2], [1], []]);
    expect(r.html).toContain("An old <span");
  });

  it("drops a leading h1 that repeats the title", () => {
    const r = markArticle(HTML, new Map([["bike", 1]]), "bike");
    expect(r.html.startsWith("<p>")).toBe(true);
    expect(r.blocks[0].first).toEqual([1]);
  });
});

describe("buildArticleBody", () => {
  const input: ArticleExportInput = {
    title: "On <bikes>",
    articleHtml: HTML,
    words: [
      { lemma: "rusty", surface: "rusty", status: "new", pos: "adj.", gloss: "生锈的", sentence: "An old bike, rusty.", hlStart: 13, hlEnd: 18, entry: { name: "Dict", html: ["<b>rusty</b>"] } },
    ],
    layout: "margin",
    style: "paperWhite",
    date: new Date(2026, 9, 29),
    options: { sentence: true, definition: true, examples: false, phonetics: true, meta: true },
    stats: { total: 8, coverage: 0.875 },
  };

  it("builds the header, notes and glossary", () => {
    const b = buildArticleBody(input);
    expect(b).toContain("On &lt;bikes&gt;");
    expect(b).toContain('data-layout="margin"');
    expect(b.match(/class="mrow"/g)?.length).toBe(3);
    expect(b).toContain('<span class="mn-zh">生锈的</span>');
    expect(b).toContain("bike, <mark>rusty</mark>.");
    expect(b).toContain("shadowrootmode");
    expect(b).toContain("88%");
  });

  it("leaves out the sentence and entry when switched off", () => {
    const b = buildArticleBody({ ...input, layout: "book", options: { ...input.options, sentence: false, definition: false } });
    expect(b).not.toContain("gl-quote");
    expect(b).not.toContain("shadowrootmode");
    expect(b).not.toContain("mrow");
  });
});
