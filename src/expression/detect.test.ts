import { describe, expect, it } from "vitest";
import { expressionMode, tooLong } from "./detect";

describe("英文怎么说：能不能用", () => {
  it("chinese_of_any_length_is_zh", () => {
    expect(expressionMode("气死我了")).toBe("zh");
    expect(expressionMode("延期")).toBe("zh");
    expect(expressionMode("催他下周前把材料送到，语气别太硬。")).toBe("zh");
  });

  it("english_sentence_is_en_but_single_word_is_not", () => {
    expect(expressionMode("I am so angry about this.")).toBe("en");
    expect(expressionMode("house")).toBeNull();
  });

  it("blank_or_punctuation_only_is_null", () => {
    expect(expressionMode("  ，。 ")).toBeNull();
    expect(expressionMode("")).toBeNull();
  });

  it("too_long_over_600_chars", () => {
    expect(tooLong("字".repeat(600))).toBe(false);
    expect(tooLong("字".repeat(601))).toBe(true);
  });
});
