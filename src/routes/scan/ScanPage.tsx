// 文章生词扫描：粘贴一篇英文 → 按词频找出超过词汇量的词，带中文释义列出来；
// 勾选的加入生词本（文中原句当语境句），认识的标记后以后不再提示。

import { useEffect, useMemo, useRef, useState } from "react";
import { wordGlosses, type WordGloss } from "../../ipc/lookup";
import { saveScannedWords, scanText, setKnownWords, type ScanStatus, type ScanWord } from "../../ipc/scan";
import { useScanPage } from "../../stores/scanPage";
import { toast } from "../../stores/toast";
import styles from "./ScanPage.module.css";
import { tr } from "../../i18n";

const LEVELS = [2000, 3000, 5000, 8000, 12000, 20000];
const LEVEL_KEY = "lens.scan.level";
const GLOSS_BATCH = 80;

interface Props {
  onLookup: (word: string) => void;
}

function readLevel(): number {
  try {
    const v = Number(localStorage.getItem(LEVEL_KEY));
    return LEVELS.includes(v) ? v : 5000;
  } catch {
    return 5000;
  }
}

function writeLevel(v: number) {
  try {
    localStorage.setItem(LEVEL_KEY, String(v));
  } catch {
    // 存不了只是下次回到默认值
  }
}

export function ScanPage({ onLookup }: Props) {
  const { text, title, result, setText, setTitle, setResult } = useScanPage();
  const [level, setLevel] = useState(readLevel);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [tab, setTab] = useState<ScanStatus>("new");
  const [sort, setSort] = useState<"order" | "rank">("order");
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [glosses, setGlosses] = useState<Map<string, WordGloss>>(new Map());
  const [focus, setFocus] = useState<string | null>(null);
  const listRef = useRef<HTMLUListElement>(null);

  const scan = async (lv = level) => {
    if (!text.trim()) return;
    setBusy(true);
    try {
      const r = await scanText(text, lv);
      setResult(r);
      setPicked(new Set(r.words.filter((w) => w.status === "new").map((w) => w.lemma)));
      setTab("new");
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  // 释义：生词和已收藏的词分批取（词性 + 本地双解词典的第一段中文）
  useEffect(() => {
    if (!result) return;
    let alive = true;
    const need = result.words.map((w) => w.lemma).filter((l) => !glosses.has(l));
    void (async () => {
      for (let i = 0; i < need.length; i += GLOSS_BATCH) {
        const batch = await wordGlosses(need.slice(i, i + GLOSS_BATCH)).catch(() => []);
        if (!alive) return;
        setGlosses((prev) => {
          const next = new Map(prev);
          for (const g of batch) next.set(g.word, g);
          return next;
        });
      }
    })();
    return () => {
      alive = false;
    };
    // glosses 不进依赖：只在扫描结果变化时补取
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [result]);

  const counts = useMemo(() => {
    const c = { new: 0, saved: 0, known: 0 };
    for (const w of result?.words ?? []) c[w.status] += 1;
    return c;
  }, [result]);

  const shown = useMemo(() => {
    const list = (result?.words ?? []).filter((w) => w.status === tab);
    return sort === "rank" ? [...list].sort((a, b) => (b.rank ?? 1e9) - (a.rank ?? 1e9)) : list;
  }, [result, tab, sort]);

  const setStatus = (lemmas: string[], status: ScanStatus) => {
    if (!result) return;
    const set = new Set(lemmas);
    setResult({ ...result, words: result.words.map((w) => (set.has(w.lemma) ? { ...w, status } : w)) });
    setPicked((prev) => new Set([...prev].filter((l) => !set.has(l))));
  };

  const markKnown = async (lemmas: string[], known: boolean) => {
    try {
      await setKnownWords(lemmas, known);
      setStatus(lemmas, known ? "known" : "new");
    } catch (e) {
      setError(String(e));
    }
  };

  const savePicked = async () => {
    if (!result) return;
    const words = result.words.filter((w) => picked.has(w.lemma) && w.status === "new");
    try {
      const n = await saveScannedWords(
        words.map((w) => ({
          lemma: w.lemma,
          surface: w.forms[0] ?? w.lemma,
          sentence: w.sentence,
          hlStart: w.hlStart,
          hlEnd: w.hlEnd,
          brief: glosses.get(w.lemma)?.gloss ?? null,
        })),
        title,
      );
      setStatus(
        words.map((w) => w.lemma),
        "saved",
      );
      toast(tr("已加入生词本：{n} 个词", { n }));
    } catch (e) {
      setError(String(e));
    }
  };

  // 点文中的词：右边列表切到它所在的分组并滚到那一行
  const focusWord = (w: ScanWord) => {
    setTab(w.status);
    setFocus(w.lemma);
    requestAnimationFrame(() => listRef.current?.querySelector(`[data-lemma="${CSS.escape(w.lemma)}"]`)?.scrollIntoView({ block: "nearest" }));
  };

  const article = useMemo(() => {
    if (!result) return null;
    const out: React.ReactNode[] = [];
    let pos = 0;
    result.tokens.forEach((t, i) => {
      const w = result.words[t.word];
      if (!w) return;
      if (t.start > pos) out.push(text.slice(pos, t.start));
      out.push(
        <button key={i} type="button" className={styles.tok} data-status={w.status} data-focus={focus === w.lemma || undefined} onClick={() => focusWord(w)} onDoubleClick={() => onLookup(w.lemma)}>
          {text.slice(t.start, t.end)}
        </button>,
      );
      pos = t.end;
    });
    out.push(text.slice(pos));
    return out;
    // focusWord 每次渲染都新建，但只用到稳定的 setter
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [result, text, focus, onLookup]);

  if (!result) {
    return (
      <div className={styles.column}>
        <header className={styles.head}>
          <h1 className={styles.title}>{tr("文章生词扫描")}</h1>
        </header>
        <p className={styles.sub}>{tr("粘贴一篇英文文章，找出超过你词汇量的词，带中文释义列出来；勾选的一键加入生词本，文中的原句会作为语境句。")}</p>
        <label className={styles.field}>
          <span>{tr("来源 / 标题（可选，收藏时记作语境的来源）")}</span>
          <input type="text" value={title} onChange={(e) => setTitle(e.target.value)} placeholder={tr("如：The Economist · 2026-10-27")} />
        </label>
        <label className={styles.field}>
          <span className="sr-only">{tr("文章")}</span>
          <textarea className={styles.paste} value={text} onChange={(e) => setText(e.target.value)} placeholder={tr("在这里粘贴文章…")} spellCheck={false} />
        </label>
        {error && (
          <p role="alert" className={styles.error}>
            {error}
          </p>
        )}
        <div className={styles.bar}>
          <label className={styles.level}>
            {tr("词汇量")}
            <select
              value={level}
              onChange={(e) => {
                const v = Number(e.target.value);
                setLevel(v);
                writeLevel(v);
              }}
            >
              {LEVELS.map((l) => (
                <option key={l} value={l}>
                  {tr("最常用的 {n} 词", { n: l })}
                </option>
              ))}
            </select>
          </label>
          <button type="button" className={styles.primary} disabled={!text.trim() || busy} onClick={() => void scan()}>
            {busy ? tr("扫描中…") : tr("扫描")}
          </button>
        </div>
      </div>
    );
  }

  const pickedNew = shown.filter((w) => picked.has(w.lemma));

  return (
    <div className={styles.wide}>
      <header className={styles.head}>
        <h1 className={styles.title}>{tr("文章生词扫描")}</h1>
        <span className={styles.stats}>
          {tr("{total} 词 · {unique} 个不同的词 · 生词 {n} 个 · 认识 {pct}%", {
            total: result.total,
            unique: result.unique,
            n: counts.new,
            pct: Math.round(result.coverage * 100),
          })}
        </span>
        <label className={styles.level}>
          {tr("词汇量")}
          <select
            value={level}
            onChange={(e) => {
              const v = Number(e.target.value);
              setLevel(v);
              writeLevel(v);
              void scan(v);
            }}
          >
            {LEVELS.map((l) => (
              <option key={l} value={l}>
                {tr("最常用的 {n} 词", { n: l })}
              </option>
            ))}
          </select>
        </label>
        <button type="button" className={styles.ghost} onClick={() => setResult(null)}>
          {tr("换一篇")}
        </button>
      </header>
      {result.source === "builtin" && <p className={styles.faint}>{tr("词频用的是内置词频表（wordfreq）；在 设置 › AI 例句 里导入 COCA 词频表后改用 COCA。")}</p>}
      {error && (
        <p role="alert" className={styles.error}>
          {error}
        </p>
      )}

      <div className={styles.split}>
        <article className={styles.article} aria-label={tr("文章")}>
          {article}
        </article>

        <section className={styles.panel} aria-label={tr("扫描出的词")}>
          <div className={styles.panelHead}>
            <div role="group" aria-label={tr("分组")} className={styles.segmented}>
              {(["new", "saved", "known"] as const).map((s) => (
                <button key={s} type="button" aria-pressed={tab === s} onClick={() => setTab(s)}>
                  {s === "new" ? tr("生词") : s === "saved" ? tr("已收藏") : tr("认识")} {counts[s]}
                </button>
              ))}
            </div>
            <button type="button" className={styles.link} onClick={() => setSort(sort === "order" ? "rank" : "order")}>
              {sort === "order" ? tr("按出现顺序") : tr("难的在前")}
            </button>
          </div>

          {tab === "new" && shown.length > 0 && (
            <div className={styles.bulk}>
              <label className={styles.check}>
                <input
                  type="checkbox"
                  checked={pickedNew.length === shown.length}
                  ref={(el) => {
                    if (el) el.indeterminate = pickedNew.length > 0 && pickedNew.length < shown.length;
                  }}
                  onChange={(e) => setPicked(e.target.checked ? new Set(shown.map((w) => w.lemma)) : new Set())}
                />
                {tr("全选")}
              </label>
              <button type="button" className={styles.primarySmall} disabled={pickedNew.length === 0} onClick={() => void savePicked()}>
                {tr("加入生词本（{n}）", { n: pickedNew.length })}
              </button>
              <button type="button" className={styles.ghost} disabled={pickedNew.length === 0} onClick={() => void markKnown(pickedNew.map((w) => w.lemma), true)}>
                {tr("标为认识")}
              </button>
            </div>
          )}

          {shown.length === 0 ? (
            <p className={styles.empty}>{tab === "new" ? tr("没有超过词汇量的生词。") : tr("这一组是空的。")}</p>
          ) : (
            <ul ref={listRef} className={styles.list}>
              {shown.map((w) => {
                const g = glosses.get(w.lemma);
                return (
                  <li key={w.lemma} className={styles.row} data-lemma={w.lemma} data-focus={focus === w.lemma || undefined}>
                    {tab === "new" && (
                      <input
                        type="checkbox"
                        aria-label={tr("选中 {w}", { w: w.lemma })}
                        checked={picked.has(w.lemma)}
                        onChange={(e) =>
                          setPicked((prev) => {
                            const next = new Set(prev);
                            if (e.target.checked) next.add(w.lemma);
                            else next.delete(w.lemma);
                            return next;
                          })
                        }
                      />
                    )}
                    <div className={styles.rowMain}>
                      <div className={styles.rowLine}>
                        <button type="button" className={styles.word} onClick={() => onLookup(w.lemma)}>
                          {w.lemma}
                        </button>
                        {g?.pos && <span className={styles.pos}>{g.pos}</span>}
                        {w.count > 1 && <span className={styles.faint}>×{w.count}</span>}
                        {w.forms.some((f) => f !== w.lemma) && <span className={styles.faint}>{w.forms.filter((f) => f !== w.lemma).join(" · ")}</span>}
                      </div>
                      <div className={styles.gloss}>{g ? (g.gloss ?? "—") : "…"}</div>
                    </div>
                    <span className={styles.rank} title={tr("词频排名")}>
                      {w.rank ? `#${w.rank}` : tr("罕见")}
                    </span>
                    {w.status === "new" && (
                      <button type="button" className={styles.link} onClick={() => void markKnown([w.lemma], true)}>
                        {tr("认识")}
                      </button>
                    )}
                    {w.status === "known" && (
                      <button type="button" className={styles.link} onClick={() => void markKnown([w.lemma], false)}>
                        {tr("撤销")}
                      </button>
                    )}
                  </li>
                );
              })}
            </ul>
          )}
        </section>
      </div>
    </div>
  );
}
