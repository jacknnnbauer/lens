// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { wrapWords } from "../routes/reader/markdown";
import { htmlWithoutMarks, MARK_CLASS, offsetsOf, paint, plainText, unpaint } from "./paint";

function root(html: string): HTMLElement {
  const el = document.createElement("div");
  el.innerHTML = html;
  document.body.append(el);
  return el;
}

const marks = (el: HTMLElement) => [...el.querySelectorAll<HTMLElement>(`mark.${MARK_CLASS}`)];

describe("paint", () => {
  it("paints_across_inline_elements", () => {
    const el = root("<p>alpha <i>beta</i> gamma</p>");
    const text = plainText(el);
    paint(el, [{ id: 1, start: 0, end: text.length, color: "green", note: false }]);
    const ms = marks(el);
    expect(ms.length).toBe(3);
    expect(ms.map((m) => m.textContent).join("")).toBe("alpha beta gamma");
    expect(ms.every((m) => m.dataset.hl === "1" && m.dataset.color === "green")).toBe(true);
    expect(plainText(el)).toBe(text);
  });

  it("marks_the_last_piece_when_it_has_a_note", () => {
    const el = root("<p>alpha <i>beta</i> gamma</p>");
    paint(el, [{ id: 7, start: 0, end: 10, color: "yellow", note: true }]);
    const ms = marks(el);
    expect(ms.filter((m) => m.dataset.note !== undefined).length).toBe(1);
    expect(ms[ms.length - 1].dataset.note).toBeDefined();
  });

  it("offsets_round_trip", () => {
    const el = root("<p>one <b>two</b> three</p><p>four five</p>");
    const t = plainText(el);
    const r = document.createRange();
    const b = el.querySelector("b")!.firstChild!;
    const p2 = el.querySelectorAll("p")[1].firstChild!;
    r.setStart(b, 1);
    r.setEnd(p2, 4);
    const [s, e] = offsetsOf(el, r)!;
    expect(t.slice(s, e)).toBe("wo threefour");
    paint(el, [{ id: 2, start: s, end: e, color: "blue", note: false }]);
    expect(marks(el).map((m) => m.textContent).join("")).toBe("wo threefour");
  });

  it("overlapping_marks_both_paint", () => {
    const el = root("<p>abcdefghij</p>");
    paint(el, [
      { id: 1, start: 0, end: 6, color: "yellow", note: false },
      { id: 2, start: 3, end: 9, color: "pink", note: false },
    ]);
    const ids = new Set(marks(el).map((m) => m.dataset.hl));
    expect(ids).toEqual(new Set(["1", "2"]));
    // 重叠处最里层是后加的那条
    const inner = marks(el).find((m) => m.textContent === "def" && m.dataset.hl === "2");
    expect(inner?.parentElement?.dataset.hl).toBe("1");
    expect(plainText(el)).toBe("abcdefghij");
  });

  it("unpaint_restores_dom", () => {
    const html = "<p>alpha <i>beta</i> gamma</p>";
    const el = root(html);
    paint(el, [{ id: 1, start: 2, end: 12, color: "yellow", note: true }]);
    unpaint(el);
    expect(el.innerHTML).toBe(html);
  });

  it("plain_text_matches_wordindex", () => {
    const el = root("<p>Call me <i>Ishmael</i>. Some years ago—never mind.</p>");
    const before = plainText(el);
    wrapWords(el);
    expect(plainText(el)).toBe(before);
    const s = before.indexOf("years ago");
    paint(el, [{ id: 3, start: s, end: s + 9, color: "yellow", note: false }]);
    expect(marks(el).map((m) => m.textContent).join("")).toBe("years ago");
    expect(el.querySelectorAll(".lens-w").length).toBeGreaterThan(5);
  });
});

describe("htmlWithoutMarks", () => {
  it("export_html_has_no_marks", () => {
    const el = root("<p>alpha <i>beta</i> gamma</p>");
    paint(el, [{ id: 1, start: 0, end: 10, color: "pink", note: true }]);
    const out = htmlWithoutMarks(el);
    expect(out).toBe("<p>alpha <i>beta</i> gamma</p>");
    expect(el.querySelectorAll("mark").length).toBeGreaterThan(0);
  });
});
