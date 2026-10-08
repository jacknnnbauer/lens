// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { buildExportBody, COPYRIGHT, safeFileName, trimEntry, type ExportInput } from "./template";

const base: ExportInput = {
  word: "colour <x>",
  date: new Date(2026, 9, 7),
  layout: "cards",
  style: "paperWhite",
  options: { phonetics: true, examples: true, context: true, sourceDate: true },
  cards: [
    { id: "a", name: "朗文", entries: [`<span class="phon">/ˈkʌlə/</span><span class="x">ex</span><a data-sound="u" href="#">🔊</a><a href="#" data-entry="hue">hue</a>`] },
    { id: "b", name: "牛津", entries: ["<p>one</p>", "<p>two</p>"] },
  ],
  context: { id: 1, sentence: "They made colour <bold>.", hlStart: 10, hlEnd: 16, app: "msedge.exe", windowTitle: "Art", url: "https://e.com/a" },
};

const parse = (html: string) => {
  const d = document.createElement("div");
  d.innerHTML = html;
  return d;
};

describe("buildExportBody", () => {
  it("renders the cards layout with escaped text, dict cards and context", () => {
    const body = buildExportBody(base);
    expect(body).toContain("<h1>colour &lt;x&gt;</h1>");
    expect(body).toContain("Lens 词镜 · 2026-10-07");
    expect(body.match(/<article class="card">/g)).toHaveLength(2);
    expect(body.match(/shadowrootmode="open" shadowrootserializable/g)).toHaveLength(3);
    expect(body).toContain("They made <mark>colour</mark> &lt;bold&gt;.");
    expect(body).toContain('<a href="https://e.com/a">');
    expect(body).toContain(COPYRIGHT);
    expect(body).not.toContain("🔊");
  });

  it("renders the document layout with numbered sections", () => {
    const body = buildExportBody({ ...base, layout: "document" });
    expect(body).toContain('data-layout="document"');
    expect(body).toContain('<span class="no">01</span>朗文');
    expect(body).toContain('<span class="no">02</span>牛津');
    expect(body).toContain("收录词典：朗文 · 牛津");
    expect(body).toContain("我的语境");
  });

  it("respects the options", () => {
    const body = buildExportBody({ ...base, options: { phonetics: false, examples: false, context: false, sourceDate: false } });
    expect(body).not.toMatch(/ˈkʌlə|>ex<|<mark>|2026-10-07/);
    expect(buildExportBody({ ...base, style: "inkSaving" })).toContain('data-style="ink"');
  });
});

describe("trimEntry", () => {
  it("unwraps in-app lookup links", () => {
    const d = parse(trimEntry(`<a href="#" data-entry="hue">hue</a>`, base.options));
    expect(d.querySelector("a")).toBeNull();
    expect(d.textContent).toBe("hue");
  });

  it("drops stray href on non-links but keeps stylesheets", () => {
    const d = parse(trimEntry(`<link rel="stylesheet" href="x.css"><span href="y" class="xr">see</span>`, base.options));
    expect(d.querySelector("link")?.getAttribute("href")).toBe("x.css");
    expect(d.querySelector("span")?.hasAttribute("href")).toBe(false);
  });
});

describe("safeFileName", () => {
  it("cleans Windows-invalid names", () => {
    expect(safeFileName('résumé: a/b?*"<>|')).toBe("resume_ a_b______");
    expect(safeFileName(" ..con.. ")).toBe("con_");
    expect(safeFileName("...")).toBe("lens");
    expect(safeFileName("x".repeat(200))).toHaveLength(120);
  });
});
