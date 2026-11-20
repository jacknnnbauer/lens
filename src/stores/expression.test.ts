import { beforeEach, describe, expect, it, vi } from "vitest";

const result = (tag: string) => ({ expression: { items: [{ style: "简单", en: tag, note: "" }] }, model: "m", cachedAt: null, costUsd: 0.001 });
let hasKey = true;
let fail = false;
const resolvers: Record<string, () => void> = {};
const generated: string[] = [];
let cache: Record<string, ReturnType<typeof result>> = {};
vi.mock("../ipc/expression", () => ({
  expressionGenerate: vi.fn(
    (input: string) =>
      new Promise((res, rej) => {
        generated.push(input);
        resolvers[input] = () => (fail ? rej("还没有填写 DeepSeek API Key") : res(result(input)));
      }),
  ),
  expressionCached: vi.fn(async (input: string) => cache[input] ?? null),
}));
vi.mock("../ipc/ai", () => ({ aiKeyStatus: vi.fn(async () => hasKey) }));

const { useExpression } = await import("./expression");
const s = () => useExpression.getState();
const flush = () => new Promise((r) => setTimeout(r, 0));

beforeEach(() => {
  hasKey = true;
  fail = false;
  generated.length = 0;
  cache = {};
  s().reset();
});

describe("英文怎么说 store", () => {
  it("stale_result_is_dropped", async () => {
    const a = s().run("A", null);
    const b = s().run("B", null);
    resolvers.B();
    await b;
    resolvers.A();
    await a;
    expect(s().key).toBe("B\u0000");
    expect(s().result?.expression.items[0].en).toBe("B");
  });

  it("nokey_when_no_key", async () => {
    fail = true;
    hasKey = false;
    const p = s().run("气死我了", null);
    resolvers["气死我了"]();
    await p;
    expect(s().status).toBe("nokey");
  });

  it("show_uses_cache_without_generate", async () => {
    cache["气死我了"] = result("cached");
    await s().show("气死我了", null);
    expect(s().status).toBe("done");
    expect(s().result?.expression.items[0].en).toBe("cached");
    await s().show("延期", null);
    await flush();
    expect(s().status).toBe("idle");
    expect(generated).toEqual([]);
  });
});
