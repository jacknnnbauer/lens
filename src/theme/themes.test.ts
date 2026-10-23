import { describe, expect, it } from "vitest";
import { allThemes, contrastIssues, contrastRatio, resolveTheme, THEME_VARS, type CustomTheme, type ThemeChoice } from "./themes";

const base: ThemeChoice = {
  theme: "paper",
  followSystem: false,
  lightTheme: "light",
  darkTheme: "dark",
  accent: "blue",
};

const custom: CustomTheme = {
  id: "custom-abc",
  name: "Tea",
  dark: true,
  colors: Object.fromEntries(THEME_VARS.map((v) => [v, "#000000"])) as CustomTheme["colors"],
};

describe("resolveTheme", () => {
  it("uses the explicit theme when not following system", () => {
    expect(resolveTheme({ ...base, theme: "dark" }, false)).toEqual({ theme: "dark", accent: "blue" });
    expect(resolveTheme({ ...base, theme: "midnight" }, false).theme).toBe("midnight");
  });

  it("follows system light / dark", () => {
    const a = { ...base, followSystem: true, darkTheme: "moss" };
    expect(resolveTheme(a, false).theme).toBe("light");
    expect(resolveTheme(a, true).theme).toBe("moss");
  });

  it("falls back on unknown ids", () => {
    expect(resolveTheme({ ...base, theme: "neon", accent: "neon" }, false)).toEqual({ theme: "paper", accent: "blue" });
    expect(resolveTheme({ ...base, followSystem: true, darkTheme: "custom-gone" }, true).theme).toBe("dark");
  });

  it("accepts custom themes that exist", () => {
    expect(resolveTheme({ ...base, theme: "custom-abc" }, false, [custom]).theme).toBe("custom-abc");
    expect(resolveTheme({ ...base, theme: "custom-abc" }, false, []).theme).toBe("paper");
  });
});

describe("themes", () => {
  it("lists 14 built-ins before customs", () => {
    const all = allThemes([custom]);
    expect(all).toHaveLength(15);
    expect(all.filter((t) => t.accentable).map((t) => t.id)).toEqual(["paper", "light", "dark"]);
    expect(all[14].custom).toBe(custom);
  });

  it("computes WCAG contrast", () => {
    expect(contrastRatio("#000000", "#FFFFFF")).toBeCloseTo(21, 1);
    expect(contrastRatio("#777777", "#777777")).toBeCloseTo(1, 5);
    expect(contrastRatio("red", "#FFFFFF")).toBeNull();
    // 全黑：每一对都是 1:1
    expect(contrastIssues(custom.colors)).toHaveLength(7);
  });
});
