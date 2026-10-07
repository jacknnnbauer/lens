import { describe, expect, it } from "vitest";
import { resolveTheme, type ThemeChoice } from "./themes";

const base: ThemeChoice = {
  theme: "paper",
  followSystem: false,
  lightTheme: "light",
  darkTheme: "dark",
  accent: "blue",
};

describe("resolveTheme", () => {
  it("uses the explicit theme when not following system", () => {
    expect(resolveTheme({ ...base, theme: "dark" }, false)).toEqual({ theme: "dark", accent: "blue" });
  });

  it("follows system light / dark", () => {
    const a = { ...base, followSystem: true };
    expect(resolveTheme(a, false).theme).toBe("light");
    expect(resolveTheme(a, true).theme).toBe("dark");
  });

  it("falls back on unknown ids", () => {
    expect(resolveTheme({ ...base, theme: "midnight", accent: "neon" }, false)).toEqual({
      theme: "paper",
      accent: "blue",
    });
    expect(resolveTheme({ ...base, followSystem: true, darkTheme: "moss" }, true).theme).toBe("dark");
  });
});
