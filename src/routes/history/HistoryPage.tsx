// 历史（SPEC 9.1 /history、M4）：按天分组；有语境句的显示原句与来源，网页来源可「回到原处」。

import { openUrl } from "@tauri-apps/plugin-opener";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { clearHistory, deleteHistory, listHistory, type HistoryItem } from "../../ipc/history";
import { useBatch } from "../../stores/batch";
import styles from "./HistoryPage.module.css";

const PAGE = 200;
const SEARCH_DEBOUNCE_MS = 200;

interface Props {
  onLookup: (word: string) => void;
  /** 页面是否可见；切回来时刷新 */
  active: boolean;
}

function dayKey(ms: number): string {
  const d = new Date(ms);
  return `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
}

function dayLabel(ms: number): string {
  const d = new Date(ms);
  const today = new Date();
  const yesterday = new Date(today.getFullYear(), today.getMonth(), today.getDate() - 1);
  if (dayKey(ms) === dayKey(today.getTime())) return "今天";
  if (dayKey(ms) === dayKey(yesterday.getTime())) return "昨天";
  const week = "日一二三四五六"[d.getDay()];
  const sameYear = d.getFullYear() === today.getFullYear();
  return `${sameYear ? "" : `${d.getFullYear()} 年 `}${d.getMonth() + 1} 月 ${d.getDate()} 日 · 周${week}`;
}

function time(ms: number): string {
  const d = new Date(ms);
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

function canOpen(url: string | null): url is string {
  return !!url && /^https?:\/\//i.test(url);
}

export function HistoryPage({ onLookup, active }: Props) {
  const [items, setItems] = useState<HistoryItem[]>([]);
  const [more, setMore] = useState(false);
  const [search, setSearch] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirmClear, setConfirmClear] = useState(false);
  const seq = useRef(0);

  const load = useCallback(async (q: string, offset: number) => {
    const my = ++seq.current;
    setLoading(true);
    try {
      const page = await listHistory({ search: q, limit: PAGE, offset });
      if (my !== seq.current) return;
      setItems((prev) => (offset === 0 ? page : [...prev, ...page]));
      setMore(page.length === PAGE);
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      if (my === seq.current) setLoading(false);
    }
  }, []);

  // 切回本页或搜索词变化时重新加载
  useEffect(() => {
    if (!active) return;
    const t = setTimeout(() => void load(search, 0), search ? SEARCH_DEBOUNCE_MS : 0);
    return () => clearTimeout(t);
  }, [active, search, load]);

  const groups = useMemo(() => {
    const out: Array<{ key: string; label: string; items: HistoryItem[] }> = [];
    for (const it of items) {
      const key = dayKey(it.createdAt);
      const last = out[out.length - 1];
      if (last?.key === key) last.items.push(it);
      else out.push({ key, label: dayLabel(it.createdAt), items: [it] });
    }
    return out;
  }, [items]);

  const remove = async (id: number) => {
    try {
      await deleteHistory([id]);
      setItems((prev) => prev.filter((i) => i.id !== id));
    } catch (e) {
      setError(String(e));
    }
  };

  const clearAll = async () => {
    try {
      await clearHistory();
      setItems([]);
      setConfirmClear(false);
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className={styles.column}>
      <header className={styles.head}>
        <h1 className={styles.title}>历史</h1>
        <label className={styles.search}>
          <span className="sr-only">搜索历史</span>
          <input type="search" placeholder="搜索查过的词" value={search} onChange={(e) => setSearch(e.target.value)} spellCheck={false} />
        </label>
        {items.length > 0 && (
          <button type="button" className={styles.ghost} onClick={() => useBatch.getState().openDialog("history")}>
            批量导出…
          </button>
        )}
        {items.length > 0 &&
          (confirmClear ? (
            <span className={styles.confirm}>
              <button type="button" className={styles.danger} onClick={() => void clearAll()}>
                确认清空全部
              </button>
              <button type="button" className={styles.ghost} onClick={() => setConfirmClear(false)}>
                取消
              </button>
            </span>
          ) : (
            <button type="button" className={styles.ghost} onClick={() => setConfirmClear(true)}>
              清空…
            </button>
          ))}
      </header>

      {error && (
        <p role="alert" className={styles.error}>
          {error}
        </p>
      )}

      {!loading && items.length === 0 && (
        <p className={styles.empty}>{search ? `没有匹配「${search}」的记录。` : "还没有查询记录。查过的词会出现在这里。"}</p>
      )}

      {groups.map((g) => (
        <section key={g.key} className={styles.group} aria-label={g.label}>
          <h2 className={styles.day}>{g.label}</h2>
          <ul className={styles.list}>
            {g.items.map((it) => (
              <li key={it.id} className={styles.row}>
                <span className={styles.time}>{time(it.createdAt)}</span>
                <div className={styles.main}>
                  <div className={styles.line}>
                    <button type="button" className={styles.word} onClick={() => onLookup(it.query)}>
                      {it.query}
                    </button>
                    <span className={styles.badge}>{it.source === "popup" ? "划词" : "主窗口"}</span>
                    {it.lookups > 1 && <span className={styles.count}>共查 {it.lookups} 次</span>}
                  </div>
                  {it.context && (
                    <>
                      <p className={styles.sentence}>
                        {it.context.sentence.slice(0, it.context.hlStart)}
                        <mark>{it.context.sentence.slice(it.context.hlStart, it.context.hlEnd)}</mark>
                        {it.context.sentence.slice(it.context.hlEnd)}
                      </p>
                      <div className={styles.source}>
                        <span className={styles.sourceText}>
                          {[it.context.app, it.context.windowTitle].filter(Boolean).join(" · ")}
                        </span>
                        {canOpen(it.context.url) && (
                          <a
                            href={it.context.url}
                            className={styles.back}
                            title={it.context.url}
                            onClick={(e) => {
                              e.preventDefault();
                              void openUrl(it.context?.url ?? "");
                            }}
                          >
                            回到原处 ↗
                          </a>
                        )}
                      </div>
                    </>
                  )}
                </div>
                <button type="button" className={styles.remove} aria-label={`删除「${it.query}」这条记录`} onClick={() => void remove(it.id)}>
                  ×
                </button>
              </li>
            ))}
          </ul>
        </section>
      ))}

      {more && (
        <button type="button" className={styles.more} disabled={loading} onClick={() => void load(search, items.length)}>
          {loading ? "加载中…" : "加载更多"}
        </button>
      )}
    </div>
  );
}
