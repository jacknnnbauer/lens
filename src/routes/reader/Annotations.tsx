// 阅读批注的界面：选区上方的颜色条、点标注弹出的小卡片（换色、笔记、删除）、右栏的批注列表。

import { useEffect, useRef, useState } from "react";
import { tr } from "../../i18n";
import { ANNOTATION_COLORS, type Annotation, type AnnotationColor } from "../../ipc/annotations";
import { isoMinute } from "../../export-templates/template";
import { useAnnotations } from "../../stores/annotations";
import { toastUndo, type BarState, type CardState } from "./useReaderAnnotations";
import styles from "./Annotations.module.css";

const COLOR_NAME: Record<AnnotationColor, string> = { yellow: "黄色", green: "绿色", blue: "蓝色", pink: "粉色" };

function Dots({ value, onPick }: { value?: AnnotationColor; onPick: (c: AnnotationColor) => void }) {
  return (
    <>
      {ANNOTATION_COLORS.map((c) => (
        <button
          key={c}
          type="button"
          className={styles.dot}
          data-color={c}
          aria-pressed={value === c}
          aria-label={tr(COLOR_NAME[c])}
          title={tr(COLOR_NAME[c])}
          // 按下时不让选区丢掉
          onMouseDown={(e) => e.preventDefault()}
          onClick={() => onPick(c)}
        />
      ))}
    </>
  );
}

/** 选中文字后浮在选区上方 */
export function AnnotationBar({ bar, onCreate, onClose }: { bar: BarState; onCreate: (c: AnnotationColor, note: boolean) => void; onClose: () => void }) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
  return (
    <div className={styles.bar} style={{ left: bar.x, top: bar.y }} role="toolbar" aria-label={tr("标注")} data-annotation-ui="">
      <Dots onPick={(c) => onCreate(c, false)} />
      <span className={styles.sep} />
      <button type="button" className={styles.text} onMouseDown={(e) => e.preventDefault()} onClick={() => onCreate("yellow", true)}>
        {tr("笔记")}
      </button>
    </div>
  );
}

/** 点了一条标注 */
export function AnnotationCard({ card, onClose }: { card: CardState; onClose: () => void }) {
  const item = useAnnotations((s) => s.items.find((a) => a.id === card.id));
  const [editing, setEditing] = useState(card.editing || !!item?.note);
  const [draft, setDraft] = useState(item?.note ?? "");
  const box = useRef<HTMLDivElement>(null);
  const area = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    setDraft(item?.note ?? "");
  }, [item?.note]);
  useEffect(() => {
    if (card.editing) area.current?.focus();
  }, [card.editing]);

  // 失焦和点外面会先后触发：同一份内容只存一次（否则撤销要按两下）
  const lastSaved = useRef<string | null>(null);
  const save = () => {
    const clean = draft.trim() || null;
    if (!item || clean === (item.note ?? null) || clean === lastSaved.current) return;
    lastSaved.current = clean;
    void useAnnotations.getState().setNote(item.id, draft).then(() => toastUndo(tr("笔记已保存")));
  };

  // 点卡片外面、按 Esc：存好笔记再关
  useEffect(() => {
    const onDown = (e: MouseEvent) => {
      if (box.current && !box.current.contains(e.target as Node)) {
        save();
        onClose();
      }
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        save();
        onClose();
      }
    };
    window.addEventListener("mousedown", onDown, true);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onDown, true);
      window.removeEventListener("keydown", onKey);
    };
  });

  if (!item) return null;
  return (
    <div ref={box} className={styles.card} style={{ left: card.x, top: card.y }} role="dialog" aria-label={tr("批注")} data-annotation-ui="">
      <div className={styles.row}>
        <Dots value={item.color} onPick={(c) => void useAnnotations.getState().recolor(item.id, c).then(() => toastUndo(tr("已改颜色")))} />
        <span className={styles.grow} />
        {!editing && (
          <button type="button" className={styles.text} onClick={() => setEditing(true)}>
            {tr("写笔记")}
          </button>
        )}
        <button
          type="button"
          className={styles.danger}
          onClick={() => {
            void useAnnotations.getState().remove(item.id).then(() => toastUndo(tr("已删除标注")));
            onClose();
          }}
        >
          {tr("删除")}
        </button>
      </div>
      {editing && (
        <textarea
          ref={area}
          className={styles.note}
          value={draft}
          placeholder={tr("写下你的想法…")}
          rows={3}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={save}
          onKeyDown={(e) => {
            if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
              save();
              onClose();
            }
          }}
        />
      )}
      <p className={styles.meta}>{isoMinute(item.createdAt)}</p>
    </div>
  );
}

/** 右栏「批注」页：按章节列出 */
export function AnnotationList({
  chapterTitles,
  missing,
  onReveal,
  onExport,
}: {
  /** 电子书的章节名；普通文章为 null */
  chapterTitles: string[] | null;
  missing: Set<number>;
  onReveal: (a: Annotation) => void;
  onExport: () => void;
}) {
  const items = useAnnotations((s) => s.items);
  if (items.length === 0) {
    return <p className={styles.empty}>{tr("选中正文里的一段文字，点一个颜色就能标注；点「笔记」可以写下想法。")}</p>;
  }
  const groups = new Map<number, Annotation[]>();
  for (const a of items) groups.set(a.chapter, [...(groups.get(a.chapter) ?? []), a]);
  const notes = items.filter((a) => a.note).length;
  return (
    <div className={styles.list}>
      <div className={styles.listHead}>
        <span>{tr("{n} 条标注 · {m} 条笔记", { n: items.length, m: notes })}</span>
        <button type="button" className={styles.text} onClick={onExport}>
          {tr("导出 PDF")}
        </button>
      </div>
      {[...groups.entries()].map(([ch, list]) => (
        <section key={ch}>
          {chapterTitles && <h4 className={styles.chapter}>{chapterTitles[ch] ?? tr("第 {n} 节", { n: ch + 1 })}</h4>}
          {list.map((a) => (
            <button key={a.id} type="button" className={styles.item} data-color={a.color} onClick={() => onReveal(a)}>
              <q className={styles.quote}>{a.quote}</q>
              {a.note && <span className={styles.itemNote}>{a.note}</span>}
              <span className={styles.meta}>
                {isoMinute(a.createdAt)}
                {missing.has(a.id) && <em className={styles.lost}> · {tr("原文已改动")}</em>}
              </span>
            </button>
          ))}
        </section>
      ))}
    </div>
  );
}
