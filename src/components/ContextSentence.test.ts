import { describe, expect, it } from "vitest";
import type { ContextSentence } from "../ipc/capture";
import { extend } from "./ContextSentence";

const ctx: ContextSentence = {
  sentence: "They made 𝒳 tracking ubiquitous.",
  hlStart: "They made 𝒳 tracking ".length,
  hlEnd: "They made 𝒳 tracking ubiquitous".length,
  before: ["One.", "Two."],
  after: ["Three.", "Four."],
};

const hl = (r: { text: string; hlStart: number; hlEnd: number }) => r.text.slice(r.hlStart, r.hlEnd);

describe("extend", () => {
  it("keeps the highlight on the word when adding sentences", () => {
    expect(hl(extend(ctx, 0, 0))).toBe("ubiquitous");
    const r = extend(ctx, 1, 1);
    expect(r.text).toBe("Two. They made 𝒳 tracking ubiquitous. Three.");
    expect(hl(r)).toBe("ubiquitous");
  });

  it("clamps to the available neighbours", () => {
    const r = extend(ctx, 5, 9);
    expect(r.text.startsWith("One. Two. ")).toBe(true);
    expect(r.text.endsWith("Three. Four.")).toBe(true);
    expect(hl(r)).toBe("ubiquitous");
  });
});
