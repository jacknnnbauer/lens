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

  it("pages_with_their_own_margin_are_not_all_indented", () => {
    // 正文页左边距 72；致谢页整体往右缩到 95，段首再缩到 118
    const body = (n: number) => page(n, [[100, FULL], [112, FULL], [124, FULL], [136, FULL]]);
    const thanks = page(4, [
      [100, FULL, { x: 118 }],
      [112, FULL, { x: 95 }],
      [124, FULL, { x: 95 }],
      [136, "and kindness.", { x: 95 }],
      [148, FULL, { x: 118 }],
      [160, FULL, { x: 95 }],
      [172, "complete.", { x: 95 }],
    ]);
    const book = reflow([body(1), body(2), body(3), thanks], [], never);
    const paras = book.chapters.flatMap((c) => c.html.match(/<p>[^<]*<\/p>/g) ?? []);
    expect(paras.filter((p) => p.includes("kindness") || p.includes("complete")).length).toBe(2);
    expect(paras.length).toBe(3);
  });

  it("indented_block_quote_stays_one_paragraph", () => {
    const q = (y: number, s: string): Line => [y, s, { x: 102 }];
    const pages = [
      page(1, [
        [100, FULL, { x: 92 }],
        [112, FULL],
        [124, "as he wrote in his diary:"],
        q(136, "it was the kind of morning that made everyone restless and nobody"),
        q(148, "could quite say why it was so, and nobody could quite say why it"),
        q(160, "was not so."),
        [172, FULL, { x: 92 }],
        [184, FULL],
      ]),
    ];
    const paras = reflow(pages, [], never).chapters.flatMap((c) => c.html.match(/<p>[^<]*<\/p>/g) ?? []);
    expect(paras.length).toBe(3);
    expect(paras[1]).toContain("restless and nobody could quite say why it was so");
  });

  it("consecutive_dialogue_lines_at_paragraph_indent_are_separate", () => {
    // 正文 x=107，段首缩进到 122；对话一行一段
    const pages = [
      page(1, [
        [88, "and so on.", { x: 107 }],
        [100, FULL, { x: 122 }],
        [112, FULL, { x: 107 }],
        [124, "the end of it.", { x: 107 }],
        // 对话行几乎铺满（不够「短行」），只能靠缩进判断
        [136, "‘We must shift him onto the couch at once, before anyone else comes,’ said Oddball.", { x: 122 }],
        [148, "I didn’t like this idea at all. I didn’t like having to touch him, not one bit.", { x: 122 }],
        [160, "‘I think we should wait for the Police to come here first,’ I said. But Oddball", { x: 122 }],
        [172, "had already made space on the folding couch and was rolling", { x: 107 }],
        [184, "with those pale eyes of his.", { x: 107 }],
      ]),
    ];
    const paras = reflow(pages, [], never).chapters.flatMap((c) => c.html.match(/<p>[^<]*<\/p>/g) ?? []);
    expect(paras.length).toBe(5);
    expect(paras[4]).toContain("But Oddball had already made space");
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

  it("pages_before_the_first_bookmark_get_their_own_toc_entry", () => {
    const pages = [1, 2, 3].map((n) => page(n, [[100, `Text on page ${"abc"[n - 1]}.`]]));
    const book = reflow(pages, [{ title: "Chapter 1", page: 3, level: 0 }], never);
    expect(book.chapters.map((c) => [c.title, c.page])).toEqual([
      ["开头", 1],
      ["Chapter 1", 3],
    ]);
    expect(book.toc).toEqual([
      { title: "开头", level: 0, chapter: 0 },
      { title: "Chapter 1", level: 0, chapter: 1 },
    ]);
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
