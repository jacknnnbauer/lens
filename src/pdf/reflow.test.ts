import { describe, expect, it } from "vitest";
import { hyphenCandidates, isScanned, reflow, type PdfItem, type PdfPage } from "./reflow";

type Line = [y: number, text: string, opt?: { x?: number; size?: number }];

/** 一页：每行一个文字块，宽度按字数估 */
function page(n: number, lines: Line[], width = 600, height = 800): PdfPage {
  const items: PdfItem[] = lines.map(([y, str, opt]) => {
    const size = opt?.size ?? 10;
    return { str, x: opt?.x ?? 72, y, w: str.length * size * 0.5, h: size, size };
  });
  return { page: n, width, height, items };
}

const never = () => false;
const text = (html: string) => html.replace(/<[^>]+>/g, " ").replace(/\s+/g, " ").trim();
const allText = (pages: PdfPage[], isWord: (w: string) => boolean = never) =>
  reflow(pages, [], isWord)
    .chapters.map((c) => text(c.html))
    .join(" ");

/** 正文行：约 90 个字符，铺满栏宽 */
const FULL = "It was the kind of morning that made everyone restless and nobody could quite say why it was";

describe("reflow", () => {
  it("strips_running_heads_and_page_numbers", () => {
    const pages = [17, 18, 19, 20, 21].map((n, i) =>
      page(i + 1, [
        [40, "THE OLD MAN AND THE SEA"],
        [100, FULL],
        [112, "and then the boat moved on."],
        [770, String(n)],
      ]),
    );
    const t = allText(pages);
    expect(t).not.toContain("OLD MAN AND THE SEA");
    expect(t).not.toMatch(/\d/);
    expect(t).toContain("boat moved on");
  });

  it("joins_lines_into_paragraphs", () => {
    const pages = [
      page(1, [
        [100, FULL],
        [112, FULL],
        [124, FULL],
        [136, "and that was all."],
        [148, FULL, { x: 92 }],
        [160, FULL],
        [172, FULL],
        [184, "the end of it."],
      ]),
    ];
    const html = reflow(pages, [], never).chapters.map((c) => c.html).join("");
    expect(html.match(/<p>/g)?.length).toBe(2);
  });

  it("rejoins_hyphenated_words", () => {
    const pages = [page(1, [[100, "This is a trans-"], [112, "port system and a well-"], [124, "known thing."]])];
    const isWord = (w: string) => w === "transport";
    expect(hyphenCandidates(pages)).toEqual(["transport", "wellknown"]);
    const t = allText(pages, isWord);
    expect(t).toContain("transport system");
    expect(t).toContain("well-known thing");
  });

  it("normalizes_ligatures", () => {
    expect(allText([page(1, [[100, "The ﬁnal ﬂoor."]])])).toContain("The final floor.");
  });

  it("two_columns_read_left_then_right", () => {
    const left = (y: number, s: string): Line => [y, s.padEnd(40, " x"), { x: 50 }];
    const right = (y: number, s: string): Line => [y, s.padEnd(40, " y"), { x: 330 }];
    const pages = [page(1, [left(100, "A1"), right(100, "B1"), left(112, "A2"), right(112, "B2"), left(124, "A3"), right(124, "B3")])];
    const t = allText(pages);
    const at = (s: string) => t.indexOf(s);
    expect([at("A1"), at("A2"), at("A3"), at("B1"), at("B2"), at("B3")].every((v, i, a) => v >= 0 && (i === 0 || v > a[i - 1]))).toBe(true);
  });

  it("splits_by_outline", () => {
    const pages = [1, 2, 3, 4, 5, 6].map((n) => page(n, [[100, `Page ${"abcdef"[n - 1]} text here.`]]));
    const book = reflow(
      pages,
      [
        { title: "One", page: 1, level: 0 },
        { title: "Two", page: 4, level: 0 },
        { title: "Two, part B", page: 5, level: 1 },
      ],
      never,
    );
    expect(book.chapters.map((c) => [c.title, c.page])).toEqual([
      ["One", 1],
      ["Two", 4],
    ]);
    expect(book.toc).toEqual([
      { title: "One", level: 0, chapter: 0 },
      { title: "Two", level: 0, chapter: 1 },
      { title: "Two, part B", level: 1, chapter: 1 },
    ]);
    expect(text(book.chapters[1].html)).toContain("Page d");
  });

  it("splits_by_large_headings_without_outline", () => {
    const pages = [
      page(1, [[60, "Chapter One", { size: 20 }], [100, "Body of one."]]),
      page(2, [[100, "More of one."]]),
      page(3, [[60, "Chapter Two", { size: 20 }], [100, "Body of two."]]),
      page(4, [[100, "More of two."]]),
    ];
    const book = reflow(pages, [], never);
    expect(book.chapters.map((c) => [c.title, c.page])).toEqual([
      ["Chapter One", 1],
      ["Chapter Two", 3],
    ]);
    expect(book.chapters[0].html).toContain("<h2>Chapter One</h2>");
  });

  it("falls_back_to_20_pages", () => {
    const pages = Array.from({ length: 45 }, (_, i) => page(i + 1, [[100, "Plain body text without any headings at all."]]));
    expect(reflow(pages, [], never).chapters.map((c) => c.page)).toEqual([1, 21, 41]);
  });

  it("escapes_html", () => {
    expect(reflow([page(1, [[100, "a < b & c > d"]])], [], never).chapters[0].html).toContain("a &lt; b &amp; c &gt; d");
  });
});

describe("isScanned", () => {
  it("detects_scanned", () => {
    const thin = (n: number) => page(n, [[100, "abc def g"]]);
    const full = (n: number) => page(n, [[100, FULL]]);
    expect(isScanned([thin(1), thin(2), thin(3), thin(4), full(5), full(6)])).toBe(true);
    expect(isScanned([full(1), full(2), thin(3)])).toBe(false);
    expect(isScanned([])).toBe(true);
  });
});
