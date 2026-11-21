import { describe, expect, it } from "vitest";
import { ReadingTimer } from "./timer";

/** 时钟可拨的计时器：每次 tick 前把时间往前拨 1 秒 */
function setup() {
  let t = 0;
  const flushed: [number, number][] = [];
  const timer = new ReadingTimer(
    () => t,
    (s, w) => flushed.push([s, w]),
  );
  const run = (seconds: number) => {
    for (let i = 0; i < seconds; i++) {
      t += 1000;
      timer.tick();
    }
  };
  return { timer, flushed, run, at: () => t };
}

describe("阅读计时", () => {
  it("idle_over_60s_stops_counting", () => {
    const { timer, flushed, run } = setup();
    timer.activity();
    run(90); // 前 60 秒算，后 30 秒不算
    timer.stop();
    expect(flushed.reduce((a, [s]) => a + s, 0)).toBe(60);
  });

  it("activity_resumes_counting", () => {
    const { timer, flushed, run } = setup();
    timer.activity();
    run(70);
    timer.activity();
    run(10);
    timer.stop();
    expect(flushed.reduce((a, [s]) => a + s, 0)).toBe(70);
  });

  it("flushes_every_30s_and_on_stop", () => {
    const { timer, flushed, run } = setup();
    timer.activity();
    run(20);
    timer.activity();
    run(25);
    expect(flushed).toEqual([[30, 0]]);
    timer.addWords(120);
    timer.stop();
    expect(flushed).toEqual([
      [30, 0],
      [15, 120],
    ]);
    // 停了以后不再计
    run(10);
    timer.stop();
    expect(flushed.length).toBe(2);
  });

  it("hidden_or_unfocused_does_not_count", () => {
    const { timer, flushed, run } = setup();
    timer.activity();
    timer.setActive(false);
    run(20);
    timer.setActive(true);
    timer.activity();
    run(5);
    timer.stop();
    expect(flushed).toEqual([[5, 0]]);
  });
});
