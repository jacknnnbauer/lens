// 搜索框（SPEC 9.4）：100ms 防抖查词 + 前缀候选下拉。
// ↑↓ 选候选、Enter 立即查并全选（接着输入即替换）、Esc 先关下拉再清空；Tab 交给页面跳到第一张卡片。
// 输入框是单行起步、随内容长高的 textarea：长句子折行显示全，不会被挤出框外。
// 粘贴的是一整句时：句子留在框里，点句中的词就选中并查它，Shift+点 / 拖选查一段；光标移到词上按 Enter 也行。

import { forwardRef, useEffect, useId, useImperativeHandle, useRef, useState } from "react";
import { suggest } from "../ipc/lookup";
import { SearchIcon } from "./icons";
import { isSentence, wordSpan } from "./sentence";
import styles from "./SearchBox.module.css";
import { tr } from "../i18n";

const DEBOUNCE_MS = 100;

export interface SearchBoxHandle {
  focus: () => void;
  setValue: (v: string) => void;
  /** 从页面别处开始打字：用这个字符替换原来的词，光标放在末尾，照常联想与查询 */
  startTyping: (ch: string) => void;
  /** 框里是一整句时选中其中一段（区间相对去掉首尾空白后的句子） */
  selectInSentence: (start: number, end: number) => void;
}

interface Props {
  /** commit = 用户明确发起（Enter / 选候选），否则是输入防抖触发 */
  onSearch: (q: string, commit: boolean) => void;
  /** Tab（下拉未打开时）：跳到结果区 */
  onTabOut: () => void;
  /** 框里是一整句时选中了其中的词（区间相对去掉首尾空白后的句子） */
  onPickInSentence?: (sentence: string, start: number, end: number) => void;
}

export const SearchBox = forwardRef<SearchBoxHandle, Props>(function SearchBox({ onSearch, onTabOut, onPickInSentence }, ref) {
  const [value, setValue] = useState("");
  const [items, setItems] = useState<string[]>([]);
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(-1);
  const input = useRef<HTMLTextAreaElement>(null);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const suggestSeq = useRef(0);
  const listId = useId();

  useImperativeHandle(ref, () => ({
    focus: () => {
      input.current?.focus();
      input.current?.select();
    },
    setValue: (v) => {
      clearTimeout(timer.current);
      setValue(v);
      setOpen(false);
    },
    startTyping: (ch) => {
      input.current?.focus();
      onChange(ch);
      requestAnimationFrame(() => input.current?.setSelectionRange(ch.length, ch.length));
    },
    selectInSentence: (start, end) => {
      const el = input.current;
      if (!el) return;
      const lead = el.value.length - el.value.trimStart().length;
      el.focus();
      el.setSelectionRange(start + lead, end + lead);
    },
  }));

  useEffect(() => () => clearTimeout(timer.current), []);

  const runNow = (q: string) => {
    clearTimeout(timer.current);
    // 让还在路上的候选请求作废：Enter 之后下拉框不能再弹出来
    suggestSeq.current++;
    setOpen(false);
    setActive(-1);
    onSearch(q, true);
  };

  const onChange = (q: string) => {
    setValue(q);
    setActive(-1);
    clearTimeout(timer.current);
    timer.current = setTimeout(() => {
      onSearch(q, false);
      const seq = ++suggestSeq.current;
      if (onPickInSentence && isSentence(q)) {
        setItems([]);
        setOpen(false);
        return;
      }
      suggest(q, 8)
        .then((list) => {
          if (seq !== suggestSeq.current) return;
          setItems(list);
          setOpen(list.length > 0 && !(list.length === 1 && list[0] === q.trim().toLowerCase()));
        })
        .catch(() => setItems([]));
    }, DEBOUNCE_MS);
  };

  // 框里是一整句：把选区 / 光标扩到整词，选中它并交给页面去查。不是句子时返回 false
  // 点击时传入鼠标位置：中文词挨着没有空格，光标落在两个词之间时要看点的是哪个字
  const pickInSentence = (point?: { x: number; y: number }) => {
    const el = input.current;
    if (!el || !onPickInSentence || !isSentence(value)) return false;
    let [from, to] = [el.selectionStart ?? 0, el.selectionEnd ?? 0];
    if (point && from === to && from > 0 && charHit(el, from - 1, point)) from = to = from - 1;
    const span = wordSpan(value, from, to);
    if (!span) return true;
    el.setSelectionRange(span.start, span.end);
    const lead = value.length - value.trimStart().length;
    onPickInSentence(value.trim(), span.start - lead, span.end - lead);
    return true;
  };

  const pick = (s: string) => {
    setValue(s);
    runNow(s);
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.nativeEvent.isComposing) return;
    if (e.key === "ArrowDown" && items.length > 0) {
      e.preventDefault();
      setOpen(true);
      setActive((i) => (i + 1) % items.length);
    } else if (e.key === "ArrowUp" && open) {
      e.preventDefault();
      setActive((i) => (i <= 0 ? items.length - 1 : i - 1));
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (open && active >= 0) pick(items[active]);
      else if (pickInSentence()) return;
      else runNow(value);
      // 查完全选，直接输入下一个词就会替换掉
      requestAnimationFrame(() => input.current?.select());
    } else if (e.key === "Escape") {
      e.preventDefault();
      if (open) setOpen(false);
      else {
        setValue("");
        runNow("");
      }
    } else if (e.key === "Tab" && !e.shiftKey) {
      if (open) setOpen(false);
      e.preventDefault();
      onTabOut();
    }
  };

  const activeId = open && active >= 0 ? `${listId}-${active}` : undefined;

  return (
    <div className={styles.wrap}>
      <label className={styles.box} data-ui="search">
        <span className={styles.icon}>
          <SearchIcon />
        </span>
        <span className="sr-only">{tr("查词")}</span>
        <textarea
          ref={input}
          className={styles.input}
          rows={1}
          role="combobox"
          aria-expanded={open}
          aria-controls={listId}
          aria-activedescendant={activeId}
          aria-autocomplete="list"
          autoComplete="off"
          spellCheck={false}
          autoFocus
          placeholder={tr("输入单词、短语，或粘贴一句话")}
          value={value}
          // 粘贴来的换行当空格：查的是一句话 / 一个词，不是多行文本
          onChange={(e) => onChange(e.target.value.replace(/\r?\n/g, " "))}
          onKeyDown={onKeyDown}
          onMouseUp={(e) => void pickInSentence({ x: e.clientX, y: e.clientY })}
          onBlur={() => setOpen(false)}
        />
        <span className={styles.hint} aria-hidden="true">
          {value ? tr("Esc 清空") : "Ctrl+K"}
        </span>
      </label>
      {open && (
        <ul id={listId} role="listbox" aria-label={tr("候选词")} className={styles.list}>
          {items.map((s, i) => (
            <li key={s} role="presentation">
              <button
                type="button"
                id={`${listId}-${i}`}
                role="option"
                aria-selected={i === active}
                tabIndex={-1}
                className={styles.option}
                // mousedown 先于 input 的 blur，避免下拉先被关掉
                onMouseDown={(e) => {
                  e.preventDefault();
                  pick(s);
                }}
              >
                {s}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
});

/** 点的位置是不是落在第 i 个字上：用一个和输入框排版相同的隐形副本量出这个字的位置 */
function charHit(el: HTMLTextAreaElement, i: number, p: { x: number; y: number }): boolean {
  const cs = getComputedStyle(el);
  const rect = el.getBoundingClientRect();
  const mirror = document.createElement("div");
  for (const k of ["fontFamily", "fontSize", "fontWeight", "fontStyle", "letterSpacing", "lineHeight", "paddingLeft", "paddingRight", "paddingTop", "paddingBottom", "borderLeftWidth", "borderRightWidth", "borderTopWidth", "borderBottomWidth", "borderStyle", "textIndent", "wordSpacing"] as const)
    mirror.style[k] = cs[k];
  Object.assign(mirror.style, {
    position: "fixed",
    left: `${rect.left}px`,
    top: `${rect.top - el.scrollTop}px`,
    boxSizing: "border-box",
    width: `${el.clientWidth + parseFloat(cs.borderLeftWidth) + parseFloat(cs.borderRightWidth)}px`,
    whiteSpace: "pre-wrap",
    overflowWrap: "break-word",
    visibility: "hidden",
    pointerEvents: "none",
  });
  const text = document.createTextNode(el.value);
  mirror.appendChild(text);
  document.body.appendChild(mirror);
  try {
    const range = document.createRange();
    range.setStart(text, i);
    range.setEnd(text, i + 1);
    const r = range.getBoundingClientRect();
    return p.x >= r.left && p.x <= r.right && p.y >= r.top && p.y <= r.bottom;
  } finally {
    mirror.remove();
  }
}
