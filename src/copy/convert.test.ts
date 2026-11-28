// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { contextMarkdown, htmlToMarkdown, htmlToText, sourceLine } from "./convert";

const ON = { includeExamples: true, imagesAsLinks: true };

const OXFORD_LIKE = `
<link rel="stylesheet" href="http://lens.localhost/res/d/O8C.css">
<span class="hw">colour</span> <span class="phon">/ˈkʌlə(r)/</span>
<a data-sound="http://lens.localhost/res/d/uk.wav" href="#">🔊</a>
<div class="sense"><span class="num">1</span> the appearance of things
  <span class="x">What's your <b>favourite</b> colour?</span>
  <span class="x">bright colours</span>
</div>
<div class="sense">2 the use of all colours</div>
<img src="http://lens.localhost/res/d/pic.png" alt="palette">
<a href="#" data-entry="hue">hue</a>`;

describe("htmlToMarkdown", () => {
  const md = htmlToMarkdown(OXFORD_LIKE, ON);

  it("turns senses into an ordered list and examples into italic sub-items", () => {
    expect(md).toMatch(/^1\. the appearance of things/m);
    expect(md).toMatch(/^1\. the use of all colours/m);
    expect(md).toContain("   - *What's your favourite colour?*");
    expect(md).toContain("   - *bright colours*");
  });

  it("wraps phonetics in inline code and drops app-only resources", () => {
    expect(md).toContain("`/ˈkʌlə(r)/`");
    expect(md).not.toMatch(/lens\.localhost|🔊|O8C\.css/);
    expect(md).toContain("palette");
    expect(md).toContain("hue");
    expect(md).not.toContain("](#)");
  });

  it("omits examples when asked", () => {
    const short = htmlToMarkdown(OXFORD_LIKE, { ...ON, includeExamples: false });
    expect(short).not.toContain("favourite");
    expect(short).toContain("the appearance of things");
  });

  it("keeps external links and http images", () => {
    const out = htmlToMarkdown(`<a data-external href="https://e.com/x">site</a> <img src="https://e.com/a.png" alt="a">`, ON);
    expect(out).toBe("[site](https://e.com/x) [a](https://e.com/a.png)");
  });

  it("never produces indented lines that CommonMark would read as code blocks", () => {
    const html = `<p>intro</p><span class="example">loose example</span><div class="sense">1 def <span class="x">nested</span></div>`;
    const lines = htmlToMarkdown(html, ON).split("\n");
    lines.forEach((l, i) => {
      expect(l, `第 ${i + 1} 行缩进 4 格以上`).not.toMatch(/^ {4,}/);
      if (/^ {3}- /.test(l)) expect(lines[i - 1]).toMatch(/^(1\. | {3}- )/);
    });
    expect(lines).toContain("- *loose example*");
    expect(lines).toContain("   - *nested*");
  });

  it("produces valid markdown without stray blank lines", () => {
    expect(md).not.toMatch(/\n{3,}/);
    expect(md).not.toMatch(/[ \t]+\n/);
  });
});

describe("htmlToText", () => {
  it("breaks at block elements and indents examples", () => {
    const t = htmlToText(OXFORD_LIKE, ON);
    const lines = t.split("\n");
    expect(lines).toContain("  What's your favourite colour?");
    expect(lines.some((l) => l.startsWith("1 the appearance of things"))).toBe(true);
    expect(t).not.toContain("🔊");
  });
});

describe("source and context", () => {
  it("formats the source line per format", () => {
    const src = { dict: "牛津高阶", headword: "colour" };
    expect(sourceLine("md", src)).toBe("> 来源：牛津高阶 · colour");
    expect(sourceLine("text", src)).toBe("来源：牛津高阶 · colour");
    expect(sourceLine("html", src)).toContain("来源：牛津高阶 · colour");
  });

  it("matches the SPEC context quote shape and escapes markdown", () => {
    const md = contextMarkdown({
      sentence: "…made tracking ubiquitous in *modern* warehouses.",
      hlStart: "…made tracking ".length,
      hlEnd: "…made tracking ubiquitous".length,
      title: "Sensors [part 1]",
      url: "https://example.com/a",
      app: "msedge.exe",
      date: new Date(2026, 9, 7),
    });
    expect(md).toBe(
      "> …made tracking **ubiquitous** in \\*modern\\* warehouses.\n> — [Sensors \\[part 1\\]](https://example.com/a) · msedge · 2026-10-07",
    );
  });
});

// 牛津高阶这类词典：全是 span，靠 CSS 分块；这里是 visibleHtml 处理后的样子（块已换成 div）。内容是自己写的示例
const SPAN_DICT = `
<div class="top-g"><span class="h"><b>lan·tern</b></span> <span class="ei-g">/ <span class="phon-gb">ˈlæntən</span> <span class="z">; <span class="z_phon-us"><i>NAmE</i></span></span> <span class="phon-us">ˈlæntərn</span> /</span>
  <div class="pos-g"><span class="pos"><b>noun</b></span></div></div>
<div class="n-g"><span class="z_n">1</span> <span class="def-g"><span class="d">a lamp with a cover that you can carry <span class="chn">灯笼</span></span></span>
  <div class="x-g"><span class="x">She lit the <span class="cl">paper lantern</span>.</span> <span class="tx">她点亮了纸灯笼。</span></div>
  <span class="xr-g">see also <b>lamp</b></span></div>
<div class="n-g"><span class="z_n">2</span> <span class="label-g">( <span class="g"><i>especially NAmE</i></span> )</span> <span class="xr-g">= <b>torch</b></span></div>`;

describe("span-based dictionaries", () => {
  const md = htmlToMarkdown(SPAN_DICT, ON);
  const lines = md.split("\n");

  it("puts the part of speech on its own line and every numbered block becomes a sense", () => {
    expect(lines).toContain("**noun**");
    expect(lines).toContain("1. a lamp with a cover that you can carry 灯笼");
    expect(md).toMatch(/^1\. \(\*especially NAmE\*\) = \*\*torch\*\*$/m);
  });

  it("only real IPA becomes inline code; labels like NAmE stay text", () => {
    expect(md).toContain("`ˈlæntən`");
    expect(md).toContain("`ˈlæntərn`");
    expect(md).not.toContain("`NAmE`");
    expect(md).not.toMatch(/\S {2,}\S/);
  });

  it("an example group with an example inside becomes one item, not nested markers", () => {
    expect(md).toContain("   - *She lit the paper lantern. 她点亮了纸灯笼。*");
    expect(md).not.toMatch(/- \*-/);
  });

  it("plain text: no double indent for nested examples, no space inside brackets", () => {
    const t = htmlToText(SPAN_DICT, ON).split("\n");
    expect(t).toContain("  She lit the paper lantern. 她点亮了纸灯笼。");
    expect(t.some((l) => l.includes("(especially NAmE)"))).toBe(true);
    expect(t).toContain("noun");
  });
});

describe("sense numbers shown in bold, definitions in their own block", () => {
  const html = `<div class="n-g"><span class="z_n"><b>3</b></span><div class="def-g">a small light <span class="chn">小灯</span></div></div>`;
  it("Markdown drops a bold sense number", () => {
    expect(htmlToMarkdown(html, ON)).toBe("1. a small light 小灯");
  });
  it("plain text keeps the number on the definition's line", () => {
    expect(htmlToText(html, ON)).toBe("3 a small light 小灯");
  });
});
