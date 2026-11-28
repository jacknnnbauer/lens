// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { cardPayload, DEFAULT_COPY } from "./copy";
import type { Card } from "../stores/lookup";

/** 牛津高阶这类全 span、靠 CSS 分块的词条，且渲染后有被 CSS 隐藏的内容 */
const ENTRY = `<span class="hw">burn·out</span><span class="hidden" style="display:none">burn·out</span><span class="sn" style="display:block"><span class="num">1</span> the state of being tired</span><span class="sn" style="display:block"><span class="num">2</span> the point at which a <a href="#">rocket</a>has used all of its fuel</span>`;

function mount(): Card {
  const host = document.createElement("div");
  host.setAttribute("data-dict-body", "oald");
  host.attachShadow({ mode: "open" }).innerHTML = ENTRY;
  document.body.append(host);
  return {
    id: "oald",
    name: "牛津",
    entries: [{ headword: "burnout", html: ENTRY }],
  } as unknown as Card;
}

describe("cardPayload", () => {
  it("gives HTML copies the same plain-text fallback as plain-text copies", () => {
    const card = mount();
    const cfg = { ...DEFAULT_COPY, appendSource: false };
    const html = cardPayload(card, "html", cfg, false);
    const text = cardPayload(card, "text", cfg, false);
    expect(html.text).toBe(text.text);
    // 被 CSS 隐藏的词头不进纯文本
    expect(html.text.match(/burn·out/g)).toHaveLength(1);
  });
});
