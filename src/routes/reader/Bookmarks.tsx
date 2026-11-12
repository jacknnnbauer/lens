// 阅读：书签。工具栏上的「书签」按钮打开列表：顶上「在这里加书签」，下面按在书里的先后列出，点一下跳过去，× 删除。
// 读到哪里本来就会自动记住（再打开回到上次的位置）；书签是自己想多记几处（好段落、回头要查的地方）。

import { useCallback, useEffect, useRef, useState } from "react";
import { addBookmark, deleteBookmark, listBookmarks, type Bookmark } from "../../ipc/articles";
import { pressed, useKeyLabel } from "../../keys";
import { toast } from "../../stores/toast";
import { scrollRoot } from "../../windows/main/scroll";
import styles from "./ReaderPage.module.css";
import { tr } from "../../i18n";

/** 屏幕上第一段看得见的原文，当书签的名字 */
function visibleText(body: HTMLElement | null): string {
  if (!body) return "";
  const top = scrollRoot().getBoundingClientRect().top + 8;
  for (const el of Array.from(body.querySelectorAll<HTMLElement>("p, h1, h2, h3, h4, h5, h6, li, blockquote, pre, td"))) {
    if (el.getBoundingClientRect().bottom <= top) continue;
    const text = (el.innerText || el.textContent || "").replace(/\s+/g, " ").trim();
    if (text) return text;
  }
  return (body.innerText || "").replace(/\s+/g, " ").trim();
}

function currentPos(): number {
  const root = scrollRoot();
  const max = root.scrollHeight - root.clientHeight;
  return max > 0 ? Math.min(1, Math.max(0, root.scrollTop / max)) : 0;
}

interface Props {
  articleId: number;
  /** 当前章（普通文章为 0） */
  chapter: number;
  /** 电子书各章标题；普通文章为 null */
  chapterTitles: string[] | null;
  body: React.RefObject<HTMLDivElement | null>;
  onJump: (chapter: number, pos: number) => void;
}

export function Bookmarks({ articleId, chapter, chapterTitles, body, onJump }: Props) {
  const [list, setList] = useState<Bookmark[]>([]);
  const [open, setOpen] = useState(false);
  const wrap = useRef<HTMLSpanElement>(null);
  const key = useKeyLabel("bookmark");

  const reload = useCallback(() => {
    listBookmarks(articleId).then(setList, () => undefined);
  }, [articleId]);
  useEffect(reload, [reload]);

  const add = useCallback(async () => {
    try {
      const label = visibleText(body.current) || tr("（无文字）");
      await addBookmark(articleId, chapter, currentPos(), label);
      toast(tr("已加书签：{label}", { label: label.length > 30 ? `${label.slice(0, 30)}…` : label }));
      reload();
    } catch (e) {
      toast(String(e));
    }
  }, [articleId, chapter, body, reload]);

  const remove = async (id: number) => {
    await deleteBookmark(id).catch(() => undefined);
    reload();
  };

  // 快捷键（默认 Ctrl+D，可在设置里改）
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!pressed(e, "bookmark") || (e.target as HTMLElement | null)?.closest?.('[contenteditable="true"], textarea, input')) return;
      e.preventDefault();
      void add();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [add]);

  useEffect(() => {
    if (!open) return;
    const close = (e: Event) => {
      if (e instanceof KeyboardEvent ? e.key === "Escape" : !wrap.current?.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener("keydown", close);
    window.addEventListener("mousedown", close);
    return () => {
      window.removeEventListener("keydown", close);
      window.removeEventListener("mousedown", close);
    };
  }, [open]);

  return (
    <span ref={wrap} className={styles.bmWrap}>
      <button type="button" className={styles.ghost} aria-expanded={open} title={tr("书签（{key} 在当前位置加书签）", { key })} onClick={() => setOpen((o) => !o)}>
        {tr("书签")}
        {list.length > 0 && ` ${list.length}`}
      </button>
      {open && (
        <div className={styles.bmMenu} role="menu" aria-label={tr("书签")}>
          <button
            type="button"
            role="menuitem"
            className={styles.bmAdd}
            onClick={() => {
              setOpen(false);
              void add();
            }}
          >
            {tr("＋ 在这里加书签")}
            <kbd>{key}</kbd>
          </button>
          {list.length === 0 ? (
            <p className={styles.bmEmpty}>{tr("还没有书签。读到想回头看的地方，加一个书签；读到哪里会自动记住，不用为这个加。")}</p>
          ) : (
            list.map((b) => (
              <div key={b.id} className={styles.bmItem}>
                <button
                  type="button"
                  role="menuitem"
                  className={styles.bmJump}
                  onClick={() => {
                    setOpen(false);
                    onJump(b.chapter, b.pos);
                  }}
                >
                  <span className={styles.bmWhere}>
                    {chapterTitles ? `${chapterTitles[b.chapter] ?? tr("第 {n} 章", { n: b.chapter + 1 })} · ` : ""}
                    {`${Math.round(b.pos * 100)}%`}
                  </span>
                  <span className={styles.bmLabel}>{b.label}</span>
                </button>
                <button type="button" className={styles.bmDel} aria-label={tr("删除书签")} title={tr("删除书签")} onClick={() => void remove(b.id)}>
                  ×
                </button>
              </div>
            ))
          )}
        </div>
      )}
    </span>
  );
}
