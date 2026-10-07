// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { adjustContrast, contrast, fixColor, parseColor, type RGB } from "./contrast";

const PANEL_DARK: RGB = [0x1b, 0x1d, 0x21];
const RAISED_DARK: RGB = [0x23, 0x26, 0x2b];
const WHITE: RGB = [255, 255, 255];

describe("color math", () => {
  it("parses hex, rgb, rgba and space syntax", () => {
    expect(parseColor("#fff")).toEqual({ rgb: [255, 255, 255], a: 1 });
    expect(parseColor("#1B1D21")).toEqual({ rgb: [27, 29, 33], a: 1 });
    expect(parseColor("rgb(10, 20, 30)")).toEqual({ rgb: [10, 20, 30], a: 1 });
    expect(parseColor("rgba(10, 20, 30, 0.5)")).toEqual({ rgb: [10, 20, 30], a: 0.5 });
    expect(parseColor("rgb(10 20 30 / 25%)")).toEqual({ rgb: [10, 20, 30], a: 0.25 });
    expect(parseColor("transparent")).toBeNull();
  });

  it("computes WCAG contrast", () => {
    expect(contrast([0, 0, 0], WHITE)).toBeCloseTo(21, 1);
    expect(contrast(WHITE, WHITE)).toBeCloseTo(1, 5);
  });

  it("lightens dark text on a dark panel, keeping the hue", () => {
    const navy: RGB = [0x1a, 0x3a, 0x8a];
    const fixed = fixColor(navy, PANEL_DARK, 4.5);
    expect(fixed).not.toBeNull();
    expect(contrast(fixed as RGB, PANEL_DARK)).toBeGreaterThanOrEqual(4.5);
    const [r, g, b] = fixed as RGB;
    expect(b).toBeGreaterThan(r); // 仍然是蓝色
    expect(b).toBeGreaterThan(g);
  });

  it("darkens light-grey text on white and leaves good colors alone", () => {
    const grey: RGB = [0x99, 0x99, 0x99];
    expect(contrast(fixColor(grey, WHITE, 4.5) as RGB, WHITE)).toBeGreaterThanOrEqual(4.5);
    expect(fixColor([0, 0, 0], WHITE, 4.5)).toBeNull();
  });
});

describe("adjustContrast", () => {
  it("fixes low-contrast text and replaces light blocks in dark themes", () => {
    const root = document.createElement("div");
    root.innerHTML = `
      <p style="color: rgb(26, 58, 138)">navy text</p>
      <div style="background-color: rgb(248, 249, 255)"><span style="color: rgb(68, 68, 68)">on a light block</span></div>
      <p style="color: rgb(236, 234, 228)">already fine</p>`;
    document.body.append(root);
    const n = adjustContrast(root, { panel: PANEL_DARK, raised: RAISED_DARK, dark: true });
    const [navy, block, span, fine] = [
      root.querySelector("p") as HTMLElement,
      root.querySelector("div") as HTMLElement,
      root.querySelector("span") as HTMLElement,
      root.querySelectorAll("p")[1] as HTMLElement,
    ];
    expect(block.style.getPropertyValue("background-color")).toBe("var(--raised)");
    // 各自对照实际所在的背景：深蓝段落在面板上，span 在被替换成 --raised 的底块里
    for (const [el, bg] of [[navy, PANEL_DARK], [span, RAISED_DARK]] as const) {
      const c = parseColor(el.style.getPropertyValue("color"));
      expect(c && contrast(c.rgb, bg)).toBeGreaterThanOrEqual(4.5);
    }
    expect(fine.style.getPropertyValue("color")).toBe("rgb(236, 234, 228)");
    expect(n).toBe(3);
  });

  it("does not replace backgrounds in light themes", () => {
    const root = document.createElement("div");
    root.innerHTML = `<div style="background-color: rgb(255, 255, 200)"><b style="color: rgb(0, 0, 0)">x</b></div>`;
    document.body.append(root);
    expect(adjustContrast(root, { panel: WHITE, raised: [240, 239, 235], dark: false })).toBe(0);
  });
});
