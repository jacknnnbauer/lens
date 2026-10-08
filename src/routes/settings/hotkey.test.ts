import { describe, expect, it } from "vitest";
import { formatHotkey } from "./hotkey";

const k = (code: string, mods: Partial<Record<"ctrlKey" | "altKey" | "shiftKey" | "metaKey", boolean>> = {}) => ({
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  metaKey: false,
  code,
  ...mods,
});

describe("formatHotkey", () => {
  it("waits while only modifiers are held", () => {
    expect(formatHotkey(k("ControlLeft", { ctrlKey: true }))).toBeNull();
    expect(formatHotkey(k("AltRight", { altKey: true }))).toBeNull();
  });

  it("formats in modifier order with readable key names", () => {
    expect(formatHotkey(k("KeyL", { altKey: true, ctrlKey: true }))).toEqual({ ok: true, value: "Ctrl+Alt+L" });
    expect(formatHotkey(k("Digit3", { ctrlKey: true, shiftKey: true }))).toEqual({ ok: true, value: "Ctrl+Shift+3" });
    expect(formatHotkey(k("Space", { ctrlKey: true, altKey: true }))).toEqual({ ok: true, value: "Ctrl+Alt+Space" });
    expect(formatHotkey(k("F8"))).toEqual({ ok: true, value: "F8" });
  });

  it("rejects bare keys, Shift-only combos, reserved and unsupported keys", () => {
    expect(formatHotkey(k("KeyA"))).toMatchObject({ ok: false });
    expect(formatHotkey(k("KeyA", { shiftKey: true }))).toMatchObject({ ok: false });
    expect(formatHotkey(k("Space", { altKey: true }))).toMatchObject({ ok: false });
    expect(formatHotkey(k("Enter", { ctrlKey: true }))).toMatchObject({ ok: false });
  });
});
