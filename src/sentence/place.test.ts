import { describe, expect, it } from "vitest";
import { placePieces } from "./place";

const S = "It was the kind of morning that made everyone restless, though nobody could say why.";
const at = (s: string) => [S.indexOf(s), S.indexOf(s) + s.length];

describe("placePieces", () => {
  it("places_in_order", () => {
    expect(placePieces(S, ["It", "was", "the kind of morning"])).toEqual([at("It"), at("was"), at("the kind of morning")]);
  });

  it("picks_next_occurrence", () => {
    const s = "the cat saw the dog and the bird";
    expect(placePieces(s, ["the cat", "the dog", "the bird"])).toEqual([
      [0, 7],
      [12, 19],
      [24, 32],
    ]);
    // 片段顺序乱了（从句列表不一定按主干顺序）：往后找不到就从头再找
    expect(placePieces(s, ["the dog", "the cat"])).toEqual([
      [12, 19],
      [0, 7],
    ]);
  });

  it("matches_despite_quotes_and_case", () => {
    const s = "She said, “I’ll be right back,” and left.";
    const r = placePieces(s, ["\"I'll be right back,\"", "AND LEFT"]);
    expect(s.slice(...(r[0] as [number, number]))).toBe("“I’ll be right back,”");
    expect(s.slice(...(r[1] as [number, number]))).toBe("and left");
  });

  it("tolerates_extra_whitespace", () => {
    const s = "a  long\nsentence here";
    expect(placePieces(s, ["long sentence"])).toEqual([[3, 16]]);
  });

  it("missing_piece_is_null", () => {
    expect(placePieces(S, ["it was a morning", ""])).toEqual([null, null]);
  });
});
