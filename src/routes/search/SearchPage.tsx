// 反查与正文搜索：中文 → 英文词（汇总各双解词典的中文释义），或在词条正文（释义、例句）里搜一段文字。
// 两者共用每部词典一份的全文索引，在本页底部管理。

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  fulltextBuild,
  fulltextCancel,
  fulltextDelete,
  fulltextSearch,
  fulltextStatus,
  hasCjk,
  onFulltextProgress,
  reverseLookup,
  type FtGroup,
  type FtStatus,
  type ReverseWord,
} from "../../ipc/fulltext";
import { useSearchPage } from "../../stores/searchPage";
import styles from "./SearchPage.module.css";
import { tr } from "../../i18n";

const DEBOUNCE_MS = 250;
const POS_ORDER = ["adj.", "n.", "v.", "adv."];

interface Props {
  onLookup: (word: string) => void;
  active: boolean;
}

function mb(bytes: number): string {
  return bytes >= 1e9 ? `${(bytes / 1e9).toFixed(1)} GB` : `${Math.max(0.1, bytes / 1e6).toFixed(1)} MB`;
}

/** 常用度：排名越靠前越常用 */
function rankLabel(rank: number | null): string | null {
  if (rank == null) return null;
  if (rank <= 3000) return tr("常用");
  if (rank <= 8000) return tr("较常用");
  return null;
}

/** 释义里标出查询的那段 */
function Marked({ text, q }: { text: string; q: string }) {
  const i = q ? text.indexOf(q) : -1;
  if (i < 0) return <>{text}</>;
  return (
    <>
      {text.slice(0, i)}
      <mark>{text.slice(i, i + q.length)}</mark>
      {text.slice(i + q.length)}
    </>
  );
}

function ReverseRow({ w, q, onLookup }: { w: ReverseWord; q: string; onLookup: (word: string) => void }) {
  const common = rankLabel(w.rank);
  return (
    <li className={styles.row}>
      <button type="button" className={styles.word} onClick={() => onLookup(w.word)}>
        {w.word}
      </button>
      {w.pos && <span className={styles.pos}>{w.pos}</span>}
      <span className={styles.gloss}>
        <Marked text={w.gloss} q={q} />
      </span>
      <span className={styles.meta}>
        {common && <span className={styles.badge}>{common}</span>}
        <span title={w.dicts.join("\n")}>{tr("{n} 部词典", { n: w.dicts.length })}</span>
      </span>
    </li>
  );
}

export function SearchPage({ onLookup, active }: Props) {
  const { query, mode, setQuery, setMode } = useSearchPage();
  const [grouped, setGrouped] = useState(false);
  const [reverse, setReverse] = useState<{ q: string; words: ReverseWord[]; indexed: number } | null>(null);
  const [groups, setGroups] = useState<{ q: string; groups: FtGroup[] } | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<FtStatus[]>([]);
  const [manage, setManage] = useState(false);
  const input = useRef<HTMLInputElement>(null);
  const seq = useRef(0);

  const refreshStatus = useCallback(() => {
    fulltextStatus().then(setStatus, (e) => setError(String(e)));
  }, []);

  useEffect(() => {
    if (!active) return;
    refreshStatus();
    requestAnimationFrame(() => input.current?.focus());
  }, [active, refreshStatus]);

  // 建索引的进度：只更新那一行；建好、出错、取消时整体刷新
  useEffect(() => {
    const off = onFulltextProgress((e) => {
      if (e.state === "building") {
        setStatus((prev) => prev.map((s) => (s.dictId === e.dictId ? { ...s, state: "building", done: e.done, total: e.total } : s)));
      } else {
        if (e.state === "error" && e.error) setError(e.error);
        refreshStatus();
      }
    });
    return () => void off.then((f) => f());
  }, [refreshStatus]);

  const ready = status.filter((s) => s.state === "ready").length;
  const busy = status.some((s) => s.state === "building" || s.state === "queued");

  const run = useCallback(async (q: string, m: "reverse" | "text") => {
    const my = ++seq.current;
    if (!q.trim()) {
      setReverse(null);
      setGroups(null);
      return;
    }
    setLoading(true);
    try {
      if (m === "reverse") {
        const r = await reverseLookup(q, 120);
        if (my === seq.current) setReverse({ q, ...r });
      } else {
        const g = await fulltextSearch(q);
        if (my === seq.current) setGroups({ q, groups: g });
      }
      if (my === seq.current) setError(null);
    } catch (e) {
      if (my === seq.current) setError(String(e));
    } finally {
      if (my === seq.current) setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (!active) return;
    const t = setTimeout(() => void run(query, mode), DEBOUNCE_MS);
    return () => clearTimeout(t);
  }, [active, query, mode, run, ready]);

  const loadMore = async (g: FtGroup) => {
    if (!groups) return;
    try {
      const [more] = await fulltextSearch(groups.q, g.dictId, g.hits.length);
      if (!more) return;
      setGroups((prev) =>
        prev && { ...prev, groups: prev.groups.map((x) => (x.dictId === g.dictId ? { ...x, hits: [...x.hits, ...more.hits] } : x)) },
      );
    } catch (e) {
      setError(String(e));
    }
  };

  const byPos = useMemo(() => {
    const words = reverse?.words ?? [];
    const key = (w: ReverseWord) => {
      const first = w.pos?.split("/")[0] ?? "";
      return POS_ORDER.includes(first) ? first : "";
    };
    return [...POS_ORDER, ""]
      .map((p) => ({ pos: p, words: words.filter((w) => key(w) === p) }))
      .filter((g) => g.words.length > 0);
  }, [reverse]);

  const posName: Record<string, string> = { "adj.": tr("形容词"), "n.": tr("名词"), "v.": tr("动词"), "adv.": tr("副词"), "": tr("其他") };
  const noIndex = status.length > 0 && ready === 0;

  return (
    <div className={styles.column}>
      <header className={styles.head}>
        <h1 className={styles.title}>{tr("反查与全文")}</h1>
        <div role="group" aria-label={tr("搜索方式")} className={styles.segmented}>
          <button type="button" aria-pressed={mode === "reverse"} onClick={() => setMode("reverse")}>
            {tr("反查（中 → 英）")}
          </button>
          <button type="button" aria-pressed={mode === "text"} onClick={() => setMode("text")}>
            {tr("正文搜索")}
          </button>
        </div>
      </header>

      <label className={styles.search}>
        <span className="sr-only">{tr(mode === "reverse" ? "输入中文" : "输入要在词条正文里搜的文字")}</span>
        <input
          ref={input}
          type="search"
          value={query}
          placeholder={tr(mode === "reverse" ? "输入中文，如「普遍存在」「放弃」" : "搜释义和例句，如 take for granted、随处可见")}
          spellCheck={false}
          onChange={(e) => {
            const v = e.target.value;
            setQuery(v);
            // 正文搜索也能搜中文；只有从空白开始输入时才按内容自动切换
            if (!query.trim() && v.trim()) setMode(hasCjk(v) ? "reverse" : "text");
          }}
        />
      </label>

      {error && (
        <p role="alert" className={styles.error}>
          {error}
        </p>
      )}

      {noIndex && (
        <section className={styles.intro}>
          <p>
            {tr("反查和正文搜索要先给词典建全文索引：每部词典一次，之后搜索是即时的。索引大约是词典文件的 1～2 倍大，建一部要十几秒到一分钟。")}
          </p>
          <p>{tr("反查用的是双解词典里的中文释义，建议先建英汉双解词典。")}</p>
          <button type="button" className={styles.primary} onClick={() => setManage(true)}>
            {tr("选择词典建立索引…")}
          </button>
        </section>
      )}

      {mode === "reverse" && reverse && reverse.q === query && ready > 0 && (
        <section aria-live="polite" className={styles.results}>
          <div className={styles.summary}>
            <span>
              {reverse.words.length === 0
                ? tr("没有释义里含「{q}」的英文词", { q: reverse.q })
                : tr("{n} 个词 · 来自 {d} 部词典", { n: reverse.words.length, d: reverse.indexed })}
            </span>
            {reverse.words.length > 0 && (
              <div role="group" aria-label={tr("排列方式")} className={styles.segmentedSmall}>
                <button type="button" aria-pressed={!grouped} onClick={() => setGrouped(false)}>
                  {tr("按贴合度")}
                </button>
                <button type="button" aria-pressed={grouped} onClick={() => setGrouped(true)}>
                  {tr("按词性")}
                </button>
              </div>
            )}
          </div>
          {grouped ? (
            byPos.map((g) => (
              <section key={g.pos} className={styles.group} aria-label={posName[g.pos]}>
                <h2 className={styles.groupHead}>
                  {posName[g.pos]} <small>{g.words.length}</small>
                </h2>
                <ul className={styles.list}>
                  {g.words.map((w) => (
                    <ReverseRow key={w.word} w={w} q={reverse.q} onLookup={onLookup} />
                  ))}
                </ul>
              </section>
            ))
          ) : (
            reverse.words.length > 0 && (
              <ul className={styles.list}>
                {reverse.words.map((w) => (
                  <ReverseRow key={w.word} w={w} q={reverse.q} onLookup={onLookup} />
                ))}
              </ul>
            )
          )}
        </section>
      )}

      {mode === "text" && groups && groups.q === query && ready > 0 && (
        <section aria-live="polite" className={styles.results}>
          <div className={styles.summary}>
            <span>
              {groups.groups.length === 0
                ? tr("词条正文里没有「{q}」", { q: groups.q })
                : tr("{n} 部词典里出现", { n: groups.groups.length })}
            </span>
          </div>
          {groups.groups.map((g) => (
            <section key={g.dictId} className={styles.group} aria-label={g.name}>
              <h2 className={styles.groupHead}>
                {g.name} <small>{tr("{n} 条", { n: g.total })}</small>
              </h2>
              <ul className={styles.list}>
                {g.hits.map((h, i) => (
                  <li key={`${h.headword}-${i}`} className={styles.hit}>
                    <button type="button" className={styles.word} onClick={() => onLookup(h.headword)}>
                      {h.headword}
                    </button>
                    {h.snippet && (
                      <p className={styles.snippet}>
                        {h.snippet.before}
                        <mark>{h.snippet.hit}</mark>
                        {h.snippet.after}
                      </p>
                    )}
                  </li>
                ))}
              </ul>
              {g.hits.length < g.total && (
                <button type="button" className={styles.more} onClick={() => void loadMore(g)}>
                  {tr("再显示 20 条（还有 {n} 条）", { n: g.total - g.hits.length })}
                </button>
              )}
            </section>
          ))}
        </section>
      )}

      {loading && <p className={styles.faint}>{tr("搜索中…")}</p>}

      <details className={styles.manage} open={manage || busy || undefined} onToggle={(e) => setManage((e.target as HTMLDetailsElement).open)}>
        <summary>
          {tr("全文索引")}
          <span className={styles.faint}> · {tr("{ready}/{total} 部词典已建", { ready, total: status.length })}</span>
        </summary>
        <div className={styles.manageBar}>
          <button
            type="button"
            className={styles.ghost}
            disabled={status.every((s) => s.state !== "none")}
            onClick={() => void fulltextBuild(status.filter((s) => s.state === "none").map((s) => s.dictId)).then(refreshStatus, (e) => setError(String(e)))}
          >
            {tr("为未建的词典全部建立")}
          </button>
          {busy && (
            <button type="button" className={styles.ghost} onClick={() => void fulltextCancel().then(refreshStatus)}>
              {tr("停止")}
            </button>
          )}
        </div>
        <ul className={styles.indexList}>
          {status.map((s) => (
            <li key={s.dictId} className={styles.indexRow}>
              <span className={styles.indexName}>{s.name}</span>
              <span className={styles.indexState}>
                {s.state === "ready" && tr("已建 · {size}", { size: mb(s.bytes) })}
                {s.state === "queued" && tr("排队中")}
                {s.state === "building" && (
                  <>
                    <progress max={s.total || 1} value={s.done} aria-label={tr("建立进度")} />
                    {s.total > 0 ? `${Math.floor((s.done / s.total) * 100)}%` : ""}
                  </>
                )}
                {s.state === "none" && tr("未建 · 词典 {size}", { size: mb(s.dictBytes) })}
              </span>
              <span className={styles.indexActions}>
                {(s.state === "none" || s.state === "ready") && (
                  <button type="button" className={styles.link} onClick={() => void fulltextBuild([s.dictId]).then(refreshStatus, (e) => setError(String(e)))}>
                    {s.state === "ready" ? tr("重建") : tr("建立")}
                  </button>
                )}
                {s.state === "ready" && (
                  <button type="button" className={styles.link} onClick={() => void fulltextDelete(s.dictId).then(refreshStatus, (e) => setError(String(e)))}>
                    {tr("删除")}
                  </button>
                )}
              </span>
            </li>
          ))}
        </ul>
        <p className={styles.faint}>{tr("索引放在 Lens 的数据目录里；词典从词典库删除时一起删除。")}</p>
      </details>
    </div>
  );
}
