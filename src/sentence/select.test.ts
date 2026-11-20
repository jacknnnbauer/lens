import { describe, expect, it } from "vitest";
import { breakdownFit, contextAround } from "./select";

describe("breakdownFit", () => {
  it("too_short_under_six_words", () => {
    expect(breakdownFit("Call me Ishmael today.")).toBe("short");
    expect(breakdownFit("It was the kind of morning that made everyone restless.")).toBe("ok");
  });
  it("too_long_over_three_sentences_or_600_chars", () => {
    expect(breakdownFit("One two three four. Five six seven eight. Nine ten eleven twelve. Thirteen fourteen.")).toBe("long");
    expect(breakdownFit(("word ").repeat(130))).toBe("long");
    expect(breakdownFit("Mr. Reed said it was late. She left the room at once.")).toBe("ok");
  });
});

describe("contextAround", () => {
  const T = "First one here. The long sentence is this one, with a clause. Then a final one. Extra.";
  it("takes_previous_and_next_sentence", () => {
    const s = T.indexOf("The long");
    const e = T.indexOf("clause.") + "clause.".length;
    expect(contextAround(T, s, e)).toEqual({ sentence: "The long sentence is this one, with a clause.", before: "First one here.", after: "Then a final one." });
  });
  it("edges_have_no_context", () => {
    expect(contextAround("Only this one here.", 0, 19)).toEqual({ sentence: "Only this one here.", before: null, after: null });
  });
});
