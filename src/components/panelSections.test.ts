import { describe, expect, it } from "vitest";
import { normalizePanel } from "./panelSections";

describe("右侧栏内容", () => {
  it("jump_is_first_for_new_installs", () => {
    expect(normalizePanel(undefined).map((p) => p.id)).toEqual(["jump", "coca", "vocab", "ai", "query", "dicts"]);
  });

  it("old_settings_get_jump_at_the_top_shown", () => {
    const old = [
      { id: "dict:oald", show: true },
      { id: "coca", show: false },
      { id: "vocab", show: true },
      { id: "ai", show: true },
      { id: "query", show: true },
      { id: "dicts", show: true },
    ];
    const out = normalizePanel(old);
    expect(out[0]).toEqual({ id: "jump", show: true });
    expect(out.slice(1)).toEqual(old);
  });

  it("user_order_and_hidden_jump_are_kept", () => {
    const mine = [
      { id: "coca", show: true },
      { id: "jump", show: false },
    ];
    expect(normalizePanel(mine).slice(0, 2)).toEqual(mine);
  });
});
