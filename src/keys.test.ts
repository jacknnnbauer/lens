import { describe, expect, it } from "vitest";
import { checkAppKey, comboOf, keyFor, showCombo } from "./keys";

const ev = (code: string, m: { ctrl?: boolean; shift?: boolean; alt?: boolean } = {}) => ({
  code,
  ctrlKey: !!m.ctrl,
  shiftKey: !!m.shift,
  altKey: !!m.alt,
  metaKey: false,
});
const t = (s: string, v?: Record<string, string>) => s.replace(/\{(\w+)\}/g, (_, k: string) => v?.[k] ?? "");

describe("app shortcuts", () => {
  it("formats presses and falls back to defaults", () => {
    expect(comboOf(ev("KeyS", { ctrl: true, shift: true }))).toBe("Ctrl+Shift+S");
    expect(comboOf(ev("Comma", { ctrl: true }))).toBe("Ctrl+Comma");
    expect(keyFor("save", {})).toBe("Ctrl+S");
    expect(keyFor("save", { save: "Alt+S" })).toBe("Alt+S");
    expect(keyFor("save", { save: null as unknown as string })).toBe("Ctrl+S");
    expect(showCombo("Ctrl+Shift+Backslash")).toBe("Ctrl+Shift+\\");
  });

  it("rejects clashes, reserved and bare keys", () => {
    expect(checkAppKey(ev("ShiftLeft", { shift: true }), "save", {}, "Ctrl+Alt+Space", t)).toBeNull();
    expect(checkAppKey(ev("KeyS", { ctrl: true, shift: true }), "save", {}, "Ctrl+Alt+Space", t)).toEqual({ ok: true, value: "Ctrl+Shift+S" });
    expect(checkAppKey(ev("KeyS"), "save", {}, "", t)).toMatchObject({ ok: false });
    expect(checkAppKey(ev("KeyC", { ctrl: true }), "save", {}, "", t)).toMatchObject({ ok: false });
    expect(checkAppKey(ev("Digit2", { ctrl: true }), "save", {}, "", t)).toMatchObject({ ok: false });
    // Ctrl+B 默认是生词本
    expect(checkAppKey(ev("KeyB", { ctrl: true }), "save", {}, "", t)).toEqual({ ok: false, reason: "Ctrl+B 已用于「生词本」" });
    // 生词本改走后 Ctrl+B 就空出来了
    expect(checkAppKey(ev("KeyB", { ctrl: true }), "save", { wordbook: "Alt+B" }, "", t)).toMatchObject({ ok: true });
    expect(checkAppKey(ev("Space", { ctrl: true, alt: true }), "save", {}, "Ctrl+Alt+Space", t)).toMatchObject({ ok: false });
  });
});
