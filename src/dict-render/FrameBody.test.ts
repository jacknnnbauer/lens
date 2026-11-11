// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { buildFrameDoc, deferResources } from "./FrameBody";

const BASE = "http://lens.localhost/";

describe("buildFrameDoc", () => {
  const opts = {
    trigger: "dblclick" as const,
    panel: [255, 255, 255] as [number, number, number],
    raised: [240, 240, 240] as [number, number, number],
    dark: false,
    base: BASE,
  };

  it("embeds a runtime that parses on its own and the deferred entry after it", () => {
    const doc = buildFrameDoc(`<p>x</p><script>go()</script>`, ":host { color: red; }", "--text:#111;", opts);
    const runtime = /<script>([\s\S]*?)<\/script><\/head>/.exec(doc)?.[1] ?? "";
    // 只解析不执行：函数里不能引用外面的东西，toString 出来的代码必须自成一体
    expect(() => new Function(runtime)).not.toThrow();
    expect(doc).toContain("html, body { color: red; }");
    expect(doc).toContain(`<body><p>x</p><script type="text/x-lens-defer">go()</script></body>`);
    expect(doc).toContain("color-scheme:light");
  });
});

describe("deferResources", () => {
  it("parks lens resources and scripts until the protocol is ready, keeping order", () => {
    const out = deferResources(
      `<link rel="stylesheet" href="${BASE}res/d/a.css"><script src="${BASE}res/d/a.js"></script>` +
        `<p style="background:url(${BASE}res/d/b.png)">x</p><img src="${BASE}res/d/c.png"><img src="data:image/png;base64,AA">` +
        `<script type="text/template">t</script><script type="text/javascript">run()</script>`,
      BASE,
    );
    expect(out).toBe(
      `<link rel="stylesheet" data-lens-href="${BASE}res/d/a.css">` +
        `<script type="text/x-lens-defer" data-lens-src="${BASE}res/d/a.js"></script>` +
        `<p data-lens-style="background:url(${BASE}res/d/b.png)">x</p><img data-lens-src="${BASE}res/d/c.png"><img src="data:image/png;base64,AA">` +
        `<script type="text/template">t</script><script type="text/x-lens-defer" data-lens-type="text/javascript">run()</script>`,
    );
  });
});
