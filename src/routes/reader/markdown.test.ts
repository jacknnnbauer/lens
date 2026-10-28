// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { renderMarkdown, WORD_CLASS, wrapWords } from "./markdown";

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
