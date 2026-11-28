// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { basicHtml, embedImages, inlineHtml } from "./inlineHtml";

describe("inlineHtml", () => {
  const host = () => {
    const div = document.createElement("div");
    div.innerHTML = `
      <style>.hw{font-weight:bold}</style>
      <entry style="display:block"><span class="hw" style="font-weight:700;color:rgb(10, 20, 30)">run</span>
      <a data-sound="x" href="#">🔊<img class="fayin" src="http://lens.localhost/res/d/uk.png"></a>
      <span class="x" style="font-style:italic;margin-left:18px">She ran home.</span>
      <a href="#" data-entry="ran">ran</a>
      <a href="https://e.com" data-external>site</a>
      <img src="http://lens.localhost/res/d/a.png"></entry>`;
    document.body.append(div);
    return div;
  };

  it("inlines whitelisted styles and maps custom tags", () => {
    const out = inlineHtml(host(), { colors: true, includeExamples: true });
    expect(out).toContain('<span style="font-weight:bold;color:rgb(10, 20, 30)"><b>run</b></span>');
    expect(out).toMatch(/<span style="font-style:italic;margin-left:18px"><i>She ran home\.<\/i><\/span>/);
    expect(out).toMatch(/^<div/);
    expect(out).not.toMatch(/<entry|<style|🔊|class=/);
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

describe("formatting that survives editors which ignore inline styles", () => {
  it("adds <b> <i> tags next to the styles, without doubling real tags", () => {
    const div = document.createElement("div");
    div.innerHTML = `<span style="font-weight:700">a</span><b>b</b><span style="text-decoration:underline">c</span>`;
    document.body.append(div);
    const out = inlineHtml(div, { colors: true, includeExamples: true });
    expect(out).toContain('<span style="font-weight:bold"><b>a</b></span>');
    expect(out).toContain('<b style="font-weight:bold">b</b>');
    expect(out).not.toContain("<b><b>");
    // 下划线：jsdom 算不出 text-decoration，在真实应用里验证
  });
});

describe("pronunciation icons and other Lens images", () => {
  it("keeps the icon inside a sound link (without its text) and marks Lens images for embedding", () => {
    const div = document.createElement("div");
    div.innerHTML = `<a data-sound="s" href="#">🔊<img src="http://lens.localhost/res/d/uk.png"></a><img src="lens://x/y.png">`;
    document.body.append(div);
    const out = inlineHtml(div, { colors: true, includeExamples: true });
    expect(out).not.toContain("🔊");
    expect(out).toContain('<img src="http://lens.localhost/res/d/uk.png" data-embed="">');
    expect(out).toContain('<img src="lens://x/y.png" data-embed="">');
  });

  it("keeps the size the image is shown at in Lens (icons stay small)", () => {
    const div = document.createElement("div");
    div.innerHTML = '<img src="http://lens.localhost/res/d/uk.png">';
    document.body.append(div);
    const img = div.querySelector("img")!;
    img.getBoundingClientRect = () => ({ width: 15, height: 15 }) as DOMRect;
    expect(inlineHtml(div, { colors: true, includeExamples: true })).toBe('<img src="http://lens.localhost/res/d/uk.png" data-embed="" width="15" height="15">');
  });

  it("embeds marked images as data URLs and drops the ones that fail", async () => {
    const html = '<span><img src="http://lens.localhost/a.png" data-embed=""><img src="http://lens.localhost/b.png" data-embed=""><img src="https://e.com/c.png"></span>';
    const out = await embedImages(html, async (u) => (u.endsWith("a.png") ? "data:image/png;base64,AAAA" : null));
    expect(out).toBe('<span><img src="data:image/png;base64,AAAA"><img src="https://e.com/c.png"></span>');
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

describe("visibleHtml keeps layout for Markdown / plain text", () => {
  it("turns spans displayed as blocks into divs, keeping their classes", async () => {
    const { visibleHtml } = await import("./inlineHtml");
    const div = document.createElement("div");
    div.innerHTML = `<span class="n-g" style="display:block"><span class="z_n">1</span> one</span><span class="pos">noun</span>`;
    document.body.append(div);
    expect(visibleHtml(div)).toBe('<div class="n-g"><span class="z_n">1</span> one</div><span class="pos">noun</span>');
  });

  it("marks text shown bold or italic so Markdown can keep the emphasis", async () => {
    const { visibleHtml } = await import("./inlineHtml");
    const div = document.createElement("div");
    div.innerHTML = `<span class="h" style="font-weight:700">word</span> <span class="g" style="font-style:italic">label</span> <b>already</b>`;
    document.body.append(div);
    expect(visibleHtml(div)).toBe('<span class="h"><b>word</b></span> <span class="g"><i>label</i></span> <b>already</b>');
  });
});
