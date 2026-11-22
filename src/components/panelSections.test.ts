import { describe, expect, it } from "vitest";
import { normalizePanel } from "./panelSections";

describe("右侧栏内容", () => {
  it("jump_is_listed_for_new_installs", () => {
    expect(normalizePanel(undefined).map((p) => p.id)).toEqual(["coca", "vocab", "ai", "query", "dicts", "jump"]);
  });

  it("old_settings_get_jump_appended_like_other_new_items", () => {
    const old = [
      { id: "dict:oald", show: true },
      { id: "coca", show: false },
      { id: "vocab", show: true },
      { id: "ai", show: true },
      { id: "query", show: true },
      { id: "dicts", show: true },
    ];
    const out = normalizePanel(old);
    expect(out.slice(0, old.length)).toEqual(old);
    expect(out[old.length]).toEqual({ id: "jump", show: true });
  });

  it("user_order_and_hidden_jump_are_kept", () => {
    const mine = [
      { id: "jump", show: false },
      { id: "coca", show: true },
    ];
    expect(normalizePanel(mine).slice(0, 2)).toEqual(mine);
  });
});
