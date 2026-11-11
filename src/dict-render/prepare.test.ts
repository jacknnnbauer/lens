// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { lensUrl } from "./lensUrl";
import { prepareEntry } from "./prepare";

const html = (content: string) => prepareEntry({ type: "html", content }, "d1").html;
const dom = (content: string) => {
  const div = document.createElement("div");
  div.innerHTML = html(content);
  return div;
};

describe("prepareEntry", () => {
  it("keeps a script copy for the sandboxed frame only when the entry has scripts", () => {
    const plain = prepareEntry({ type: "html", content: "<p>a</p>" }, "d1");
    expect(plain.frame).toBeUndefined();
    const s = prepareEntry(
      { type: "html", content: `<script src="x.js"></script><p onclick="t()">a</p><a href="javascript:t()">b</a><img src="i.png">` },
      "d1",
    );
    // 清洗版不带脚本
    expect(s.html).not.toMatch(/script|onclick|javascript:/);
    // iframe 版保留脚本，资源改成 lens 地址
    expect(s.frame).toMatch(/<script src="[^"]*res\/d1\/x\.js"><\/script>/);
    expect(s.frame).toContain(`onclick="t()"`);
    expect(s.frame).toContain(`href="javascript:t()"`);
    expect(s.frame).toMatch(/src="[^"]*res\/d1\/i\.png"/);
  });

  it("removes scripts, event handlers and javascript: urls", () => {
    const out = html(`<p onclick="x()">a<script>alert(1)</script><a href="javascript:alert(1)">b</a></p>`);
    expect(out).not.toMatch(/script|onclick|javascript/i);
    expect(out).toContain("a");
  });

  it("keeps leading stylesheet links and rewrites them to lens urls", () => {
    const d = dom(`<link rel="stylesheet" href="LSC4.css"><head><link href="x.css" rel="icon"></head><span class="hw">run</span>`);
    const links = d.querySelectorAll("link");
    expect(links).toHaveLength(1);
    expect(links[0].getAttribute("href")).toBe(lensUrl("d1", "LSC4.css"));
    // 没有 rel=stylesheet 浏览器不会加载它（曾被 DOMPurify 剥掉）
    expect(links[0].getAttribute("rel")).toBe("stylesheet");
    expect(d.querySelector(".hw")?.textContent).toBe("run");
  });

  it("keeps inline <style> blocks", () => {
    expect(dom(`<style>a.j{color:red}</style><b>x</b>`).querySelector("style")?.textContent).toContain("a.j");
  });

  it("rewrites relative, rooted and backslash resource paths", () => {
    const d = dom(`<img src="img/a.png"><img src="/img/b.png"><img src="\\img\\c.png"><img src="../x/../d.png?v=1">`);
    const srcs = Array.from(d.querySelectorAll("img")).map((i) => i.getAttribute("src"));
    expect(srcs).toEqual([
      lensUrl("d1", "img/a.png"),
      lensUrl("d1", "img/b.png"),
      lensUrl("d1", "img/c.png"),
      lensUrl("d1", "x/d.png"),
    ]);
    expect(srcs[3]).not.toContain("..");
  });

  it("turns sound:// and entry:// into data attributes", () => {
    const d = dom(`<a href="sound://uk/run.mp3">🔊</a><a href="entry://take%20off">take off</a><a href="bword://ran#x">ran</a><a href="entry://#sense2">2</a>`);
    const [snd, e1, e2, anchor] = Array.from(d.querySelectorAll("a"));
    expect(snd.getAttribute("data-sound")).toBe(lensUrl("d1", "uk/run.mp3"));
    expect(e1.getAttribute("data-entry")).toBe("take off");
    expect(e2.getAttribute("data-entry")).toBe("ran");
    expect(anchor.getAttribute("href")).toBe("#sense2");
    expect(anchor.hasAttribute("data-entry")).toBe(false);
  });

  it("blocks remote resources but keeps external links clickable", () => {
    const d = dom(`<img src="https://tracker.example/p.gif"><link rel="stylesheet" href="http://cdn.example/a.css"><a href="https://example.com">site</a>`);
    expect(d.querySelector("img")?.hasAttribute("src")).toBe(false);
    expect(d.querySelector("link")).toBeNull();
    const a = d.querySelector("a");
    expect(a?.getAttribute("href")).toBe("https://example.com");
    expect(a?.hasAttribute("data-external")).toBe(true);
  });

  it("allows data:image but not other data urls", () => {
    const d = dom(`<img src="data:image/png;base64,AAAA"><a href="data:text/html,<script>1</script>">x</a>`);
    expect(d.querySelector("img")?.getAttribute("src")).toMatch(/^data:image\/png/);
    expect(d.querySelector("a")?.getAttribute("href")).toBeNull();
  });

  it("removes forms and iframes", () => {
    expect(html(`<form><input value=1></form><iframe src="x.html"></iframe><p>ok</p>`)).toBe("<p>ok</p>");
  });

  it("summarizes visible text only", () => {
    const { summary } = prepareEntry(
      { type: "html", content: `<style>.a{color:red}</style><b>ubiquitous</b>  <i>adj.</i>\n present everywhere` },
      "d1",
    );
    expect(summary).toBe("ubiquitous adj. present everywhere");
    const long = prepareEntry({ type: "text", content: "x".repeat(200) }, "d1").summary;
    expect(long).toHaveLength(121);
  });

  it("escapes plain-text entries", () => {
    expect(prepareEntry({ type: "text", content: "a < b & c" }, "d1").html).toBe(
      '<div class="lens-plain">a &lt; b &amp; c</div>',
    );
  });
});
