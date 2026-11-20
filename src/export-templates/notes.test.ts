import { describe, expect, it } from "vitest";
import { buildNotesBody, type NotesInput } from "./notes";

const t = (y: number, m: number, d: number, h = 9) => new Date(y, m - 1, d, h, 30).getTime();

const input = (over: Partial<NotesInput> = {}): NotesInput => ({
  title: "Hillbilly Elegy",
  author: "J. D. Vance",
  exportedAt: new Date(2026, 10, 20, 10, 0),
  chapters: [
    {
      title: "Chapter 1",
      items: [
        { quote: "Appalachia is a place of hardship.", note: "开篇点题", color: "yellow", createdAt: t(2026, 11, 2) },
        { quote: "Mamaw was a force of nature.", note: null, color: "pink", createdAt: t(2026, 11, 3) },
      ],
    },
    { title: "Chapter 3", items: [{ quote: "The most important thing.", note: "记住", color: "blue", createdAt: t(2026, 11, 19, 21) }] },
  ],
  ...over,
});

describe("buildNotesBody", () => {
  it("cover_has_title_author_counts_and_dates", () => {
    const html = buildNotesBody(input());
    expect(html).toContain("Hillbilly Elegy");
    expect(html).toContain("J. D. Vance");
    expect(html).toContain("3 条标注 · 2 条笔记");
    expect(html).toContain("2026-11-02 — 2026-11-19");
    expect(html).toContain("2026-11-20");
  });

  it("groups_by_chapter_in_order", () => {
    const html = buildNotesBody(input());
    const a = html.indexOf("Chapter 1");
    const b = html.indexOf("Appalachia");
    const c = html.indexOf("Chapter 3");
    const d = html.indexOf("The most important thing");
    expect(a >= 0 && a < b && b < c && c < d).toBe(true);
    expect(html.match(/data-color="pink"/g)?.length).toBe(1);
  });

  it("escapes_and_keeps_note_lines", () => {
    const html = buildNotesBody(
      input({ chapters: [{ title: "A & <B>", items: [{ quote: "x < y && z", note: "line one\nline <two>", color: "green", createdAt: t(2026, 11, 5) }] }] }),
    );
    expect(html).toContain("A &amp; &lt;B&gt;");
    expect(html).toContain("x &lt; y &amp;&amp; z");
    expect(html).toContain("line one<br>line &lt;two&gt;");
    expect(html).not.toContain("<B>");
  });

  it("changed_quotes_still_exported", () => {
    const html = buildNotesBody(input({ chapters: [{ title: "", items: [{ quote: "Gone sentence.", note: null, color: "yellow", createdAt: t(2026, 11, 5), missing: true }] }] }));
    expect(html).toContain("Gone sentence.");
    expect(html).toContain("原文已改动");
  });

  it("article_without_author_or_chapters", () => {
    const html = buildNotesBody(input({ author: null, chapters: [{ title: "", items: [{ quote: "Only one.", note: null, color: "yellow", createdAt: t(2026, 11, 5) }] }] }));
    expect(html).toContain("1 条标注 · 0 条笔记");
    expect(html).toContain("2026-11-05");
    expect(html).not.toContain("<h2 class=\"nt-chapter\"></h2>");
  });
});
