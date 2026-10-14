// 搜索框（SPEC 9.4）：100ms 防抖查词 + 前缀候选下拉。
// ↑↓ 选候选、Enter 立即查并全选（接着输入即替换）、Esc 先关下拉再清空；Tab 交给页面跳到第一张卡片。

import { forwardRef, useEffect, useId, useImperativeHandle, useRef, useState } from "react";
import { suggest } from "../ipc/lookup";
import { SearchIcon } from "./icons";
import styles from "./SearchBox.module.css";
import { tr } from "../i18n";

const DEBOUNCE_MS = 100;

export interface SearchBoxHandle {
  focus: () => void;
  setValue: (v: string) => void;
  /** 从页面别处开始打字：用这个字符替换原来的词，光标放在末尾，照常联想与查询 */
  startTyping: (ch: string) => void;
}

interface Props {
  /** commit = 用户明确发起（Enter / 选候选），否则是输入防抖触发 */
  onSearch: (q: string, commit: boolean) => void;
  /** Tab（下拉未打开时）：跳到结果区 */
  onTabOut: () => void;
}

export const SearchBox = forwardRef<SearchBoxHandle, Props>(function SearchBox({ onSearch, onTabOut }, ref) {
  const [value, setValue] = useState("");
  const [items, setItems] = useState<string[]>([]);
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(-1);
  const input = useRef<HTMLInputElement>(null);
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
      suggest(q, 8)
        .then((list) => {
          if (seq !== suggestSeq.current) return;
          setItems(list);
          setOpen(list.length > 0 && !(list.length === 1 && list[0] === q.trim().toLowerCase()));
        })
        .catch(() => setItems([]));
    }, DEBOUNCE_MS);
  };

  const pick = (s: string) => {
    setValue(s);
    runNow(s);
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
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
      <label className={styles.box}>
        <span className={styles.icon}>
          <SearchIcon />
        </span>
        <span className="sr-only">{tr("查词")}</span>
        <input
          ref={input}
          className={styles.input}
          type="text"
          role="combobox"
          aria-expanded={open}
          aria-controls={listId}
          aria-activedescendant={activeId}
          aria-autocomplete="list"
          autoComplete="off"
          spellCheck={false}
          autoFocus
          placeholder={tr("输入单词或短语")}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          onKeyDown={onKeyDown}
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
