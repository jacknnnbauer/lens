import { beforeEach, describe, expect, it } from "vitest";
import type { Annotation, AnnotationColor, NewAnnotation } from "../ipc/annotations";
import { setAnnotationApi, useAnnotations } from "./annotations";

// 内存里的假后端
let rows: Annotation[] = [];
let nextId = 1;
let clock = 1000;
setAnnotationApi({
  list: async (articleId) => rows.filter((r) => r.articleId === articleId),
  add: async (n: NewAnnotation) => {
    const now = ++clock;
    const row: Annotation = { ...n, id: nextId++, note: n.note?.trim() ? n.note : null, createdAt: n.createdAt ?? now, updatedAt: now };
    rows.push(row);
    return row;
  },
  setColor: async (id, color: AnnotationColor) => {
    const r = rows.find((x) => x.id === id)!;
    r.color = color;
    return { ...r };
  },
  setNote: async (id, note) => {
    const r = rows.find((x) => x.id === id)!;
    r.note = note;
    return { ...r };
  },
  remove: async (id) => {
    const r = rows.find((x) => x.id === id) ?? null;
    rows = rows.filter((x) => x.id !== id);
    return r;
  },
});

const input = (quote: string, articleId = 1): NewAnnotation => ({ articleId, chapter: 0, quote, prefix: "", suffix: "", color: "yellow", note: null, createdAt: null });
const s = () => useAnnotations.getState();

beforeEach(async () => {
  rows = [];
  nextId = 1;
  await s().load(1);
});

describe("annotations store", () => {
  it("undo_add_removes_it", async () => {
    await s().add(input("a"));
    expect(s().items.length).toBe(1);
    expect(await s().undo()).toBeTruthy();
    expect(s().items.length).toBe(0);
    expect(rows.length).toBe(0);
    expect(await s().undo()).toBeNull();
  });

  it("undo_delete_restores_everything", async () => {
    const a = await s().add({ ...input("a"), color: "pink", note: "my note" });
    const before = { ...s().items[0] };
    await s().remove(a.id);
    expect(s().items.length).toBe(0);
    await s().undo();
    const back = s().items[0];
    expect([back.quote, back.color, back.note, back.createdAt]).toEqual([before.quote, "pink", "my note", before.createdAt]);
  });

  it("undo_recolor_and_note", async () => {
    const a = await s().add(input("a"));
    await s().recolor(a.id, "green");
    await s().setNote(a.id, "n1");
    await s().setNote(a.id, "n2");
    await s().undo();
    expect(s().items[0].note).toBe("n1");
    await s().undo();
    expect(s().items[0].note).toBeNull();
    await s().undo();
    expect(s().items[0].color).toBe("yellow");
  });

  it("undo_after_restore_follows_new_id", async () => {
    const a = await s().add(input("a"));
    await s().recolor(a.id, "blue");
    await s().remove(a.id);
    await s().undo(); // 恢复（新 id）
    await s().undo(); // 撤销改色：作用在恢复后的那条上
    expect(s().items[0].color).toBe("yellow");
  });

  it("undo_stack_resets_on_article_change", async () => {
    await s().add(input("a"));
    await s().load(2);
    expect(await s().undo()).toBeNull();
  });
});
