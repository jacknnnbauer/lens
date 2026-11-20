import { beforeEach, describe, expect, it, vi } from "vitest";

const calls: string[] = [];
let hasKey = true;
let fail = false;
vi.mock("../ipc/sentence", () => ({
  sentenceBreakdown: vi.fn(async (sentence: string, _b: unknown, _a: unknown, force: boolean) => {
    calls.push(`${sentence}|${force}`);
    if (fail) throw "还没有填写 DeepSeek API Key";
    return { breakdown: { translation: "译", skeleton: [], parts: [], points: [] }, model: "m", cachedAt: null, costUsd: 0.001 };
  }),
}));
vi.mock("../ipc/ai", () => ({ aiKeyStatus: vi.fn(async () => hasKey) }));

const { useSentences } = await import("./sentences");
const s = () => useSentences.getState();

beforeEach(() => {
  calls.length = 0;
  hasKey = true;
  fail = false;
  s().reset(null);
  s().reset(1);
});

describe("sentences store", () => {
  it("no_key_shows_settings_prompt", async () => {
    fail = true;
    hasKey = false;
    await s().run("A long sentence here.", null, null);
    expect(s().items[0].status).toBe("nokey");
  });

  it("other_errors_are_shown", async () => {
    fail = true;
    await s().run("A long sentence here.", null, null);
    expect(s().items[0].status).toBe("error");
  });

  it("same_sentence_twice_is_not_requested_again", async () => {
    await s().run("One.", null, null);
    await s().run("Two.", null, null);
    await s().run("One.", null, null);
    expect(calls).toEqual(["One.|false", "Two.|false"]);
    expect(s().items.map((x) => x.sentence)).toEqual(["One.", "Two."]);
    await s().run("One.", null, null, true);
    expect(calls[2]).toBe("One.|true");
  });

  it("reset_on_article_change_only", async () => {
    await s().run("One.", null, null);
    s().reset(1);
    expect(s().items.length).toBe(1);
    s().reset(2);
    expect(s().items.length).toBe(0);
  });
});
