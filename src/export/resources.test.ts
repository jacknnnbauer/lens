// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { lensUrl } from "../dict-render/lensUrl";
import { inlineCssUrls, inlineEntry } from "./resources";

const files: Record<string, { type: string; body: string }> = {
  [lensUrl("d", "a.css")]: { type: "text/css", body: ".x{background:url('img/bg.png')} .y{background:url(https://cdn.example/a.png)}" },
  [lensUrl("d", "img/bg.png")]: { type: "image/png", body: "PNG" },
  [lensUrl("d", "pic.gif")]: { type: "image/gif", body: "GIF" },
};

vi.stubGlobal("fetch", async (url: string) => {
  const f = files[url];
  // jsdom 的 Blob 与 Node 的 Response 不兼容，用字符串 + content-type 头
  return f ? new Response(f.body, { headers: { "content-type": f.type } }) : new Response("", { status: 404 });
});

afterEach(() => vi.clearAllMocks());

describe("resource inlining", () => {
  it("rewrites lens url() in CSS relative to the stylesheet, leaves others", async () => {
    const css = await inlineCssUrls(files[lensUrl("d", "a.css")].body, lensUrl("d", "a.css"));
    expect(css).toMatch(/url\("data:image\/png;base64,/);
    expect(css).toContain("url(https://cdn.example/a.png)");
  });

  it("replaces stylesheets with inline styles and images with data URIs", async () => {
    const html = await inlineEntry(
      `<link rel="stylesheet" href="${lensUrl("d", "a.css")}"><img src="${lensUrl("d", "pic.gif")}"><img src="${lensUrl("d", "missing.png")}">`,
    );
    expect(html).toMatch(/^<style>\.x\{background:url\("data:image\/png/);
    expect(html).toMatch(/<img src="data:image\/gif;base64,[^"]+">/);
    expect(html).not.toContain("missing.png");
    expect(html).not.toContain("<link");
  });
});
