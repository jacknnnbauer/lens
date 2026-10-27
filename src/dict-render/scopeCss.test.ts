import { describe, expect, it } from "vitest";
import { absolutizeUrls, scopeCss, scopeSelector } from "./scopeCss";

const BASE = "http://lens.localhost/res/d1/css/oald.css";

describe("scopeSelector", () => {
  it("maps html / body / :root to :host", () => {
    expect(scopeSelector("body")).toBe(":host");
    expect(scopeSelector("html")).toBe(":host");
    expect(scopeSelector(":root")).toBe(":host");
    expect(scopeSelector("html body")).toBe(":host");
    expect(scopeSelector("html > body .x")).toBe(":host .x");
    expect(scopeSelector("body .sense b")).toBe(":host .sense b");
    expect(scopeSelector("body > div")).toBe(":host div");
  });

  it("keeps compounds on the host", () => {
    expect(scopeSelector("body.dark .x")).toBe(":host(.dark) .x");
    expect(scopeSelector("html#oald")).toBe(":host(#oald)");
  });

  it("leaves other selectors alone", () => {
    expect(scopeSelector(".body")).toBe(".body");
    expect(scopeSelector("tbody td")).toBe("tbody td");
    expect(scopeSelector("bodytext")).toBe("bodytext");
    expect(scopeSelector("div body")).toBe("div body");
  });
});

describe("scopeCss", () => {
  it("rewrites selector lists, recurses into @media and keeps @keyframes", () => {
    const { css } = scopeCss(
      `/* c */ html, body { font-family: Gentium; } .a, body .b { color: red }
       @media (max-width: 600px) { body { font-size: 14px } }
       @keyframes k { from { opacity: 0 } to { opacity: 1 } }`,
      BASE,
    );
    expect(css).toContain(":host, :host {");
    expect(css).toContain(".a, :host .b {");
    expect(css).toMatch(/@media \(max-width: 600px\) \{\s*:host \{ font-size: 14px \}/);
    expect(css).toContain("@keyframes k { from { opacity: 0 } to { opacity: 1 } }");
    expect(css).not.toContain("/* c */");
  });

  it("pulls @font-face out with absolute urls", () => {
    const { css, fonts } = scopeCss(`@font-face { font-family: "KK"; src: url(../fonts/kk.ttf) } .p { font-family: KK }`, BASE);
    expect(css).not.toContain("@font-face");
    expect(fonts).toHaveLength(1);
    expect(fonts[0]).toContain('url("http://lens.localhost/res/d1/fonts/kk.ttf")');
    expect(css).toContain(".p { font-family: KK }");
  });

  it("does not split on braces or commas inside strings and parens", () => {
    const { css } = scopeCss(`.x::before { content: "}{," } :is(body, p) .y { color: blue }`, BASE);
    expect(css).toContain(`.x::before { content: "}{," }`);
    expect(css).toContain(":is(body, p) .y {");
  });
});

describe("absolutizeUrls", () => {
  it("resolves relative urls and imports, keeps data and absolute ones", () => {
    const out = absolutizeUrls(`@import "base.css"; .a { background: url('img/a.png') } .b { background: url(data:image/png;base64,AA) } .c { background: url(http://x/y.png) }`, BASE);
    expect(out).toContain('@import url("http://lens.localhost/res/d1/css/base.css")');
    expect(out).toContain('url("http://lens.localhost/res/d1/css/img/a.png")');
    expect(out).toContain('url("data:image/png;base64,AA")');
    expect(out).toContain('url("http://x/y.png")');
  });

  it("resolves root-relative paths against the dictionary root", () => {
    const out = absolutizeUrls(`.a { src: url(/css/fonts/a.ttf) } .b { background: url(\\img\\b.png) }`, BASE, "http://lens.localhost/res/d1/");
    expect(out).toContain('url("http://lens.localhost/res/d1/css/fonts/a.ttf")');
    expect(out).toContain('url("http://lens.localhost/res/d1/img/b.png")');
  });
});
