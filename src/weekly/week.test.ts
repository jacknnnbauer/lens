import { describe, expect, it } from "vitest";
import { shiftWeek, weekOf } from "./week";

describe("周范围", () => {
  it("week_of_sunday_belongs_to_previous_monday", () => {
    const w = weekOf(new Date(2026, 10, 22, 23, 30)); // 2026-11-22 周日
    expect([w.fromDay, w.toDay]).toEqual(["2026-11-16", "2026-11-23"]);
    expect(w.fromMs).toBe(new Date(2026, 10, 16).getTime());
    expect(w.toMs).toBe(new Date(2026, 10, 23).getTime());
    expect([w.year, w.week]).toEqual([2026, 47]);
  });

  it("monday_midnight_starts_a_new_week", () => {
    expect(weekOf(new Date(2026, 10, 23, 0, 0)).fromDay).toBe("2026-11-23");
  });

  it("cross_year_week_uses_iso_week_number", () => {
    // 2026-12-28 周一 – 2027-01-03：ISO 2026 年第 53 周
    const w = weekOf(new Date(2027, 0, 2));
    expect([w.fromDay, w.toDay, w.year, w.week]).toEqual(["2026-12-28", "2027-01-04", 2026, 53]);
    // 2027-01-04 周一：2027 年第 1 周
    expect([weekOf(new Date(2027, 0, 4)).year, weekOf(new Date(2027, 0, 4)).week]).toEqual([2027, 1]);
  });

  it("shift_week_moves_by_whole_weeks", () => {
    const w = weekOf(new Date(2026, 10, 18));
    expect(shiftWeek(w, -1).fromDay).toBe("2026-11-09");
    expect(shiftWeek(w, 7).fromDay).toBe("2027-01-04");
  });
});
