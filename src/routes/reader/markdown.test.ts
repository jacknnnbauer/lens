// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { renderBookChapter, renderMarkdown, resolveBookPath, WORD_CLASS, wrapWords } from "./markdown";

describe("reader markdown", () => {
  const md = "# Rusty bikes\n\nAn **old** bike, [rusted](https://x.com) by rain. ![a pic](p.png)\n\n```\nlet code = 1;\n```\n\n- It's `inline` still ubiquitous.";

  it("renders without images and wraps words in order", () => {
    const root = document.createElement("div");
    root.innerHTML = renderMarkdown(md, (alt) => `[image: ${alt}]`);
    expect(root.querySelector("img")).toBeNull();
    const { plain, byStart } = wrapWords(root);
    // 块之间是换行；代码、链接、图片占位不进纯文本
    expect(plain.split("\n")[0]).toBe("Rusty bikes");
    expect(plain).not.toContain("let code");
    expect(plain).not.toContain("rusted");
    const words = [...root.querySelectorAll(`.${WORD_CLASS}`)].map((s) => s.textContent);
    expect(words).toEqual(["Rusty", "bikes", "An", "old", "bike", "by", "rain", "It's", "still", "ubiquitous"]);
    // 每个 span 的起点在纯文本里正好是这个词
    for (const [start, span] of byStart) {
      expect(plain.slice(start, start + (span.textContent ?? "").length)).toBe(span.textContent);
    }
  });
});

describe("resolveBookPath", () => {
  it("相对当前章节解析，解码，分开锚点", () => {
    expect(resolveBookPath("OEBPS/text/a.xhtml", "../img/b%20c.png")).toEqual(["OEBPS/img/b c.png", null]);
    expect(resolveBookPath("OEBPS/text/a.xhtml", "b.xhtml#n1")).toEqual(["OEBPS/text/b.xhtml", "n1"]);
    expect(resolveBookPath("OEBPS/text/a.xhtml", "#n2")).toEqual(["OEBPS/text/a.xhtml", "n2"]);
  });
});

describe("renderBookChapter", () => {
  const render = (raw: string) => renderBookChapter(raw, "OEBPS/text/a.xhtml", 7, (alt) => `[${alt}]`);

  it("按 XHTML 解析：自闭合的锚点不会把后面的内容包进去", () => {
    const tpl = document.createElement("template");
    tpl.innerHTML = render('<html xmlns="http://www.w3.org/1999/xhtml"><head><title>t</title></head><body><p><a id="n1"/>Rain fell.</p></body></html>');
    expect(tpl.content.querySelector("a")?.textContent).toBe("");
    expect(tpl.content.querySelector("p")?.textContent).toBe("Rain fell.");
    expect(tpl.content.querySelector("title")).toBeNull();
  });

  it("写得不规范的章节按 HTML 解析", () => {
    expect(render("<html><body><p>one<br>two &nbsp;<b>three</p></body></html>")).toContain("three");
  });

  it("去掉样式和脚本，图片从书里取，网络图片换占位", () => {
    const html = render('<html xmlns="http://www.w3.org/1999/xhtml"><body><style>p{}</style><p class="x" style="color:red">Hi</p><script>bad()</script><img src="../img/a%20b.png" alt="pic"/><img src="https://x.com/a.png" alt="net"/></body></html>');
    expect(html).not.toMatch(/color:red|class="x"|bad\(\)/);
    expect(html).toContain("book/7/OEBPS/img/a%20b.png");
    expect(html).toContain("[net]");
    expect(html).not.toContain("x.com");
  });

  it("svg 封面换成图片", () => {
    const html = render('<html xmlns="http://www.w3.org/1999/xhtml"><body><svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink"><image xlink:href="../c.jpg"/></svg></body></html>');
    expect(html).toContain("book/7/OEBPS/c.jpg");
    expect(html).not.toContain("svg");
  });
});
