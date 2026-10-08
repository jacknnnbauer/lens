import { describe, expect, it } from "vitest";
import { moveBefore } from "./useDragReorder";

describe("moveBefore", () => {
  const ids = ["a", "b", "c", "d"];
  it("moves down and up", () => {
    expect(moveBefore(ids, "a", 3)).toEqual(["b", "c", "a", "d"]);
    expect(moveBefore(ids, "a", 4)).toEqual(["b", "c", "d", "a"]);
    expect(moveBefore(ids, "d", 0)).toEqual(["d", "a", "b", "c"]);
    expect(moveBefore(ids, "c", 1)).toEqual(["a", "c", "b", "d"]);
  });
  it("keeps the order when dropped in place", () => {
    expect(moveBefore(ids, "b", 1)).toEqual(ids);
    expect(moveBefore(ids, "b", 2)).toEqual(ids);
    expect(moveBefore(ids, "x", 0)).toEqual(ids);
  });
});
