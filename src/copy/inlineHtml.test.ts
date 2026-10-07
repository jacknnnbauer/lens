// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { basicHtml, inlineHtml } from "./inlineHtml";

describe("inlineHtml", () => {
  const host = () => {
    const div = document.createElement("div");
    div.innerHTML = `
      <style>.hw{font-weight:bold}</style>
      <entry style="display:block"><span class="hw" style="font-weight:700;color:rgb(10, 20, 30)">run</span>
      <a data-sound="x" href="#">🔊</a>
      <span class="x" style="font-style:italic;margin-left:18px">She ran home.</span>
      <a href="#" data-entry="ran">ran</a>
      <a href="https://e.com" data-external>site</a>
      <img src="http://lens.localhost/res/d/a.png"></entry>`;
    document.body.append(div);
    return div;
  };

  it("inlines whitelisted styles and maps custom tags", () => {
    const out = inlineHtml(host(), { colors: true, includeExamples: true });
    expect(out).toContain('<span style="font-weight:bold;color:rgb(10, 20, 30)">run</span>');
    expect(out).toMatch(/<span style="font-style:italic;margin-left:18px">She ran home\.<\/span>/);
    expect(out).toMatch(/^<div/);
    expect(out).not.toMatch(/<entry|<style|🔊|lens\.localhost|class=/);
    // 链接保留 href（浏览器默认的链接颜色与正文不同，会被内联，属正常）
    expect(out).toMatch(/<a href="https:\/\/e\.com"[^>]*>site<\/a>/);
    expect(out).toContain("<span>ran</span>");
  });

  it("drops colors when asked (dark themes) and examples when excluded", () => {
    const out = inlineHtml(host(), { colors: false, includeExamples: false });
    expect(out).not.toContain("color:");
    expect(out).not.toContain("She ran home");
  });
});

describe("basicHtml", () => {
  it("keeps structure but strips styles, classes and app-only bits", () => {
    const out = basicHtml(`<div class="a" style="color:red"><b>x</b><img src="lens.png"><a href="https://e.com" class="l">y</a></div>`, {
      colors: true,
      includeExamples: true,
    });
    expect(out).toBe('<div><b>x</b><a href="https://e.com">y</a></div>');
  });
});

describe("visibleHtml", () => {
  it("drops elements hidden by CSS but keeps classes", async () => {
    const { visibleHtml } = await import("./inlineHtml");
    const div = document.createElement("div");
    div.innerHTML = `<span class="num" style="display:none">0</span><span class="x" style="color:red">shown</span>`;
    document.body.append(div);
    expect(visibleHtml(div)).toBe('<span class="x">shown</span>');
  });
});

describe("block spans", () => {
  it("turns spans displayed as blocks into divs so Word keeps them on separate lines", async () => {
    const { inlineHtml } = await import("./inlineHtml");
    const div = document.createElement("div");
    div.innerHTML = `<span class="x" style="display:block">one</span><span class="x" style="display:block">two</span><span>inline</span>`;
    document.body.append(div);
    const out = inlineHtml(div, { colors: false, includeExamples: true });
    expect(out).toBe("<div>one</div><div>two</div><span>inline</span>");
  });
});
