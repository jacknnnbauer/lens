// 源码里每个 tr("中文") 都要有英文；漏了就在这里报出来。
import { describe, expect, it } from "vitest";
import { en } from "./en";

// 用 Vite 读源码文本（项目不带 Node 类型）
const sources = import.meta.glob(["../**/*.ts", "../**/*.tsx", "!../**/*.test.ts", "!../i18n/**"], {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

describe("English table", () => {
  it("covers every tr() key in the source", () => {
    expect(Object.keys(sources).length).toBeGreaterThan(30);
    const missing = new Set<string>();
    for (const text of Object.values(sources)) {
      for (const m of text.matchAll(/\btr\(\s*"((?:[^"\\]|\\.)*)"/g)) {
        const key = JSON.parse(`"${m[1]}"`) as string;
        if (/[㐀-鿿＀-￯　-〿]/.test(key) && !(key in en)) missing.add(key);
      }
    }
    expect([...missing]).toEqual([]);
  });
});
