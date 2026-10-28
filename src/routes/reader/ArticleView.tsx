// 阅读视图：左边是排好版的文章，右边是「释义 / 生词」。
// 点一个词（或拖选一个短语）就在右边查；所在句子存为语境句，查到后自动收进生词本（可关）。
// 生词按词频标在正文上（可关），「生词」页列出全文的生词，可批量收藏、标记认识。

import { openUrl } from "@tauri-apps/plugin-opener";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { DictCard } from "../../components/DictCard";
import { acceptable, paragraphOf } from "../../components/inAppLookup";
import { openArticle, renameArticle, saveReadingContext, setArticleProgress, type Article } from "../../ipc/articles";
import { wordGlosses, type WordGloss } from "../../ipc/lookup";
import { saveScannedWords, scanText, setKnownWords, type ScanResult, type ScanStatus, type ScanWord } from "../../ipc/scan";
import { saveWord, wordId } from "../../ipc/words";
import { copyCard, DEFAULT_COPY, FORMAT_NAME } from "../../copy/copy";
import type { CopyFormat } from "../../copy/convert";
import { useLookup } from "../../stores/lookup";
import { useRoute } from "../../stores/route";
import { briefOf, isReferenceDict } from "../../stores/save";
import { useSettings } from "../../stores/settings";
import { toast } from "../../stores/toast";
import { scrollRoot } from "../../windows/main/scroll";
import { renderMarkdown, WORD_CLASS, wrapWords, type WordIndex } from "./markdown";
import styles from "./ReaderPage.module.css";
import { tr } from "../../i18n";

const LEVELS = [2000, 3000, 5000, 8000, 12000, 20000];
const GLOSS_BATCH = 80;

function readPref<T>(key: string, fallback: T, valid: (v: unknown) => boolean): T {
  try {
    const raw = localStorage.getItem(key);
    const v = raw == null ? null : JSON.parse(raw);
    return valid(v) ? (v as T) : fallback;
  } catch {
    return fallback;
  }
}

function writePref(key: string, v: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(v));
  } catch {
    // 记不住只是下次回到默认值
  }
}

interface Props {
  id: number;
  onBack: () => void;
}

interface Picked {
  word: string;
  contextId: number | null;
  at: number;
}

export function ArticleView({ id, onBack }: Props) {
  const [article, setArticle] = useState<Article | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [level, setLevel] = useState<number>(() => readPref("lens.scan.level", 5000, (v) => LEVELS.includes(v as number)));
  const [marks, setMarks] = useState<boolean>(() => readPref("lens.reader.marks", true, (v) => typeof v === "boolean"));
  const [autoSave, setAutoSave] = useState<boolean>(() => readPref("lens.reader.autoSave", true, (v) => typeof v === "boolean"));
  const [tab, setTab] = useState<"dict" | "words">("words");
  const [scan, setScan] = useState<ScanResult | null>(null);
  const [picked, setPicked] = useState<Picked | null>(null);
  const [editingTitle, setEditingTitle] = useState(false);
  const bodyRef = useRef<HTMLDivElement>(null);
  const index = useRef<WordIndex | null>(null);

  // 读文章、排版、包词，然后恢复上次读到的位置
  useEffect(() => {
    let alive = true;
    setArticle(null);
    setScan(null);
    openArticle(id).then(
      (a) => {
        if (!alive) return;
        if (!a) setError(tr("这篇文章已被删除"));
        setArticle(a);
      },
      (e) => alive && setError(String(e)),
    );
    return () => {
      alive = false;
    };
  }, [id]);

  useEffect(() => {
    const el = bodyRef.current;
    if (!article || !el) return;
    el.innerHTML = renderMarkdown(article.body, (alt) => (alt ? tr("［图片：{alt}］", { alt }) : tr("［图片］")));
    index.current = wrapWords(el);
    requestAnimationFrame(() => {
      const root = scrollRoot();
      root.scrollTo({ top: article.progress * Math.max(0, root.scrollHeight - root.clientHeight) });
    });
  }, [article]);

  const rescan = useCallback(async () => {
    const ix = index.current;
    if (!ix) return;
    try {
      setScan(await scanText(ix.plain, level));
    } catch (e) {
      setError(String(e));
    }
  }, [level]);

  useEffect(() => {
    if (article) void rescan();
  }, [article, rescan]);

  // 扫描结果标到正文的词上
  useEffect(() => {
    const el = bodyRef.current;
    const ix = index.current;
    if (!el || !ix || !scan) return;
    for (const s of Array.from(el.querySelectorAll<HTMLElement>("[data-status]"))) {
      delete s.dataset.status;
      delete s.dataset.lemma;
    }
    for (const t of scan.tokens) {
      const span = ix.byStart.get(t.start);
      const w = scan.words[t.word];
      if (span && w) {
        span.dataset.status = w.status;
        span.dataset.lemma = w.lemma;
      }
    }
  }, [scan]);

  // 记下读到哪里
  useEffect(() => {
    if (!article) return;
    const root = scrollRoot();
    let t: ReturnType<typeof setTimeout> | undefined;
    const onScroll = () => {
      clearTimeout(t);
      t = setTimeout(() => {
        const max = root.scrollHeight - root.clientHeight;
        if (max > 0) void setArticleProgress(article.id, root.scrollTop / max).catch(() => undefined);
      }, 600);
    };
    root.addEventListener("scroll", onScroll, { passive: true });
    return () => {
      clearTimeout(t);
      root.removeEventListener("scroll", onScroll);
    };
  }, [article]);

  // ── 点词查词 ──
  const lastPick = useRef<{ word: string; at: number } | null>(null);
  const pick = useCallback(
    async (word: string, range: Range) => {
      if (!article) return;
      const now = Date.now();
      // 双击时 click 和 mouseup 都会到，同一个词只查一次
      if (lastPick.current && lastPick.current.word === word && now - lastPick.current.at < 800) return;
      lastPick.current = { word, at: now };
      const para = paragraphOf(range);
      const contextId = para ? await saveReadingContext(word, para.paragraph, para.sel16, article.title).catch(() => null) : null;
      setPicked({ word, contextId, at: now });
      setTab("dict");
      void useLookup.getState().search(word, { commit: true, contextId });
    },
    [article],
  );

  const onClick = (e: React.MouseEvent) => {
    const t = e.target as HTMLElement;
    const a = t.closest("a");
    if (a) {
      e.preventDefault();
      const href = a.getAttribute("href") ?? "";
      if (/^https?:\/\//i.test(href)) void openUrl(href);
      return;
    }
    const sel = window.getSelection();
    if (sel && !sel.isCollapsed) return;
    const w = t.closest<HTMLElement>(`.${WORD_CLASS}`);
    if (!w?.textContent) return;
    const r = document.createRange();
    r.selectNodeContents(w);
    void pick(w.textContent, r);
  };

  // 拖选一个词或短语
  const onMouseUp = (e: React.MouseEvent) => {
    if (e.button !== 0 || e.detail > 1) return;
    setTimeout(() => {
      const sel = window.getSelection();
      if (!sel || sel.isCollapsed || sel.rangeCount === 0) return;
      const text = sel.toString().trim();
      if (acceptable(text, true)) void pick(text.replace(/\s+/g, " "), sel.getRangeAt(0));
    }, 0);
  };

  // 查到后自动收藏（带原句）
  const { handle, cards, done } = useLookup();
  const savedFor = useRef<number | null>(null);
  const [savedWords, setSavedWords] = useState<Set<string>>(new Set());
  const doSave = useCallback(
    async (p: Picked, quiet: boolean) => {
      try {
        // 简释优先用双解词典的中文（和生词扫描一致），没有再用首部词典的摘要
        const zh = await wordGlosses([p.word]).then((g) => g[0]?.gloss ?? null, () => null);
        await saveWord(p.word, { brief: zh ?? briefOf(useLookup.getState().cards, p.word), contextId: p.contextId });
        setSavedWords((prev) => new Set(prev).add(p.word.toLowerCase()));
        toast(quiet ? tr("已收进生词本：{w}（带原句）", { w: p.word }) : tr("已收藏「{q}」", { q: p.word }));
        void rescan();
      } catch (e) {
        toast(tr("收藏失败：{error}", { error: String(e) }));
      }
    },
    [rescan],
  );
  useEffect(() => {
    if (!picked || !autoSave || savedFor.current === picked.at) return;
    if (!done || !handle || handle.query.trim() !== picked.word) return;
    if (!cards.some((c) => c.status === "hit")) return;
    savedFor.current = picked.at;
    void doSave(picked, true);
  }, [picked, autoSave, done, handle, cards, doSave]);

  const lookupInPanel = (w: string) => {
    setPicked({ word: w, contextId: null, at: Date.now() });
    savedFor.current = null;
    setTab("dict");
    void useLookup.getState().search(w, { commit: true });
  };

  const locate = (lemma: string) => {
    const el = bodyRef.current?.querySelector<HTMLElement>(`[data-lemma="${CSS.escape(lemma)}"]`);
    if (!el) return;
    el.scrollIntoView({ block: "center", behavior: "smooth" });
    el.dataset.flash = "";
    setTimeout(() => delete el.dataset.flash, 1600);
  };

  const saveTitle = async (t: string) => {
    setEditingTitle(false);
    if (!article || !t.trim() || t.trim() === article.title) return;
    try {
      await renameArticle(article.id, t);
      setArticle({ ...article, title: t.trim() });
    } catch (e) {
      setError(String(e));
    }
  };

  const toggle = (key: string, v: boolean, set: (v: boolean) => void) => {
    set(v);
    writePref(key, v);
  };

  return (
    <div className={styles.wide}>
      <header className={styles.readerBar}>
        <button type="button" className={styles.ghost} onClick={onBack}>
          {tr("← 文章库")}
        </button>
        {editingTitle && article ? (
          <input
            className={styles.titleInput}
            defaultValue={article.title}
            autoFocus
            aria-label={tr("文章标题")}
            onBlur={(e) => void saveTitle(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void saveTitle(e.currentTarget.value);
              if (e.key === "Escape") setEditingTitle(false);
            }}
          />
        ) : (
          <button type="button" className={styles.titleBtn} title={tr("改标题")} onClick={() => setEditingTitle(true)} disabled={!article}>
            {article?.title ?? "…"}
          </button>
        )}
        <span className={styles.barSpacer} />
        <label className={styles.inline}>
          {tr("词汇量")}
          <select
            value={level}
            onChange={(e) => {
              const v = Number(e.target.value);
              setLevel(v);
              writePref("lens.scan.level", v);
            }}
          >
            {LEVELS.map((l) => (
              <option key={l} value={l}>
                {tr("最常用的 {n} 词", { n: l })}
              </option>
            ))}
          </select>
        </label>
        <label className={styles.inline}>
          <input type="checkbox" checked={marks} onChange={(e) => toggle("lens.reader.marks", e.target.checked, setMarks)} />
          {tr("标出生词")}
        </label>
        <label className={styles.inline} title={tr("点词查到后，连同所在句子收进生词本")}>
          <input type="checkbox" checked={autoSave} onChange={(e) => toggle("lens.reader.autoSave", e.target.checked, setAutoSave)} />
          {tr("查词自动收藏")}
        </label>
      </header>

      {error && (
        <p role="alert" className={styles.error}>
          {error}
        </p>
      )}

      <div className={styles.split}>
        <div className={styles.paper}>
          <div
            ref={bodyRef}
            className={styles.prose}
            data-reader=""
            data-marks={marks || undefined}
            onClick={onClick}
            onMouseUp={onMouseUp}
          />
          {article && <p className={styles.faint}>{tr("点一个词查词，拖选可以查短语。")}</p>}
        </div>

        <aside className={styles.panel} aria-label={tr("释义与生词")}>
          <div role="tablist" aria-label={tr("右侧栏")} className={styles.segmented}>
            <button type="button" role="tab" aria-selected={tab === "dict"} aria-pressed={tab === "dict"} onClick={() => setTab("dict")}>
              {tr("释义")}
            </button>
            <button type="button" role="tab" aria-selected={tab === "words"} aria-pressed={tab === "words"} onClick={() => setTab("words")}>
              {tr("生词")} {scan ? scan.words.filter((w) => w.status === "new").length : ""}
            </button>
          </div>
          {tab === "dict" ? (
            <DictPanel picked={picked} autoSave={autoSave} savedWords={savedWords} onSave={(p) => void doSave(p, false)} onLookup={lookupInPanel} />
          ) : (
            <WordsPanel
              scan={scan}
              title={article?.title ?? ""}
              onLocate={locate}
              onLookup={lookupInPanel}
              onChanged={() => void rescan()}
              onError={setError}
            />
          )}
        </aside>
      </div>
    </div>
  );
}

// ── 右侧栏：释义 ──

function DictPanel({
  picked,
  autoSave,
  savedWords,
  onSave,
  onLookup,
}: {
  picked: Picked | null;
  autoSave: boolean;
  savedWords: Set<string>;
  onSave: (p: Picked) => void;
  onLookup: (w: string) => void;
}) {
  const { handle, cards, done } = useLookup();
  const copyCfg = useSettings((s) => s.settings?.copy) ?? DEFAULT_COPY;
  const updateSettings = useSettings((s) => s.update);
  const [gloss, setGloss] = useState<WordGloss | null>(null);
  const [inBook, setInBook] = useState(false);
  const word = handle?.query.trim() ?? "";

  useEffect(() => {
    setGloss(null);
    setInBook(false);
    if (!word) return;
    let alive = true;
    wordGlosses([word]).then((g) => alive && setGloss(g[0] ?? null), () => undefined);
    wordId(word).then((id) => alive && setInBook(id !== null), () => undefined);
    return () => {
      alive = false;
    };
  }, [word, savedWords]);

  if (!picked || !handle) {
    return <p className={styles.empty}>{tr("点文章里的词，释义显示在这里。")}</p>;
  }
  // 词频、搭配类词典（COCA 之类）排到后面
  const hits = cards
    .filter((c) => c.status === "hit" || c.status === "pending")
    .sort((a, b) => Number(isReferenceDict(a.name)) - Number(isReferenceDict(b.name)));
  const isPicked = picked.word === word;

  return (
    <div className={styles.dictPanel}>
      <div className={styles.dictHead}>
        <h2>{word}</h2>
        {gloss?.pos && <span className={styles.pos}>{gloss.pos}</span>}
        <span className={styles.barSpacer} />
        {inBook ? (
          <span className={styles.savedTag}>{tr("已在生词本")}</span>
        ) : (
          <button type="button" className={styles.primarySmall} disabled={!done || hits.length === 0} onClick={() => onSave(isPicked ? picked : { word, contextId: null, at: Date.now() })}>
            {tr("收藏")}
          </button>
        )}
        <button type="button" className={styles.link} onClick={() => useRoute.getState().go("lookup")}>
          {tr("在查词页打开")}
        </button>
      </div>
      {gloss?.gloss && <p className={styles.glossLine}>{gloss.gloss}</p>}
      {!autoSave && isPicked && picked.contextId !== null && !inBook && <p className={styles.faint}>{tr("收藏时会带上这句原文。")}</p>}
      {done && hits.length === 0 && <p className={styles.empty}>{tr("本地词典里没有「{q}」", { q: word })}</p>}
      <div className={styles.cards}>
        {hits.map((c) => (
          <DictCard
            key={c.id}
            card={c}
            onEntry={onLookup}
            onToggleKeyDown={() => undefined}
            copyFormat={copyCfg.defaultFormat}
            onCopy={(f) => void copyCard(c, f, copyCfg)}
            onSetDefaultFormat={(f: CopyFormat) => {
              updateSettings({ copy: { defaultFormat: f } });
              toast(tr("默认复制格式：{format}", { format: tr(FORMAT_NAME[f]) }));
            }}
            onQueryOnline={() => void useLookup.getState().queryOnline(c.id)}
          />
        ))}
      </div>
    </div>
  );
}

// ── 右侧栏：生词 ──

function WordsPanel({
  scan,
  title,
  onLocate,
  onLookup,
  onChanged,
  onError,
}: {
  scan: ScanResult | null;
  title: string;
  onLocate: (lemma: string) => void;
  onLookup: (w: string) => void;
  onChanged: () => void;
  onError: (e: string) => void;
}) {
  const [group, setGroup] = useState<ScanStatus>("new");
  const [sort, setSort] = useState<"order" | "rank">("order");
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [glosses, setGlosses] = useState<Map<string, WordGloss>>(new Map());

  useEffect(() => {
    setPicked(new Set((scan?.words ?? []).filter((w) => w.status === "new").map((w) => w.lemma)));
  }, [scan]);

  useEffect(() => {
    if (!scan) return;
    let alive = true;
    const need = scan.words.map((w) => w.lemma).filter((l) => !glosses.has(l));
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
  }, [scan]);

  const counts = useMemo(() => {
    const c = { new: 0, saved: 0, known: 0 };
    for (const w of scan?.words ?? []) c[w.status] += 1;
    return c;
  }, [scan]);

  const shown = useMemo(() => {
    const list = (scan?.words ?? []).filter((w) => w.status === group);
    return sort === "rank" ? [...list].sort((a, b) => (b.rank ?? 1e9) - (a.rank ?? 1e9)) : list;
  }, [scan, group, sort]);

  if (!scan) return <p className={styles.empty}>{tr("扫描中…")}</p>;

  const pickedNew = shown.filter((w) => picked.has(w.lemma));

  const markKnown = async (lemmas: string[], known: boolean) => {
    try {
      await setKnownWords(lemmas, known);
      onChanged();
    } catch (e) {
      onError(String(e));
    }
  };

  const savePicked = async (words: ScanWord[]) => {
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
      toast(tr("已加入生词本：{n} 个词", { n }));
      onChanged();
    } catch (e) {
      onError(String(e));
    }
  };

  return (
    <div className={styles.wordsPanel}>
      <p className={styles.stats}>
        {tr("{total} 词 · {unique} 个不同的词 · 生词 {n} 个 · 认识 {pct}%", {
          total: scan.total,
          unique: scan.unique,
          n: counts.new,
          pct: Math.round(scan.coverage * 100),
        })}
      </p>
      <div className={styles.panelHead}>
        <div role="group" aria-label={tr("分组")} className={styles.segmentedSmall}>
          {(["new", "saved", "known"] as const).map((s) => (
            <button key={s} type="button" aria-pressed={group === s} onClick={() => setGroup(s)}>
              {s === "new" ? tr("生词") : s === "saved" ? tr("已收藏") : tr("认识")} {counts[s]}
            </button>
          ))}
        </div>
        <button type="button" className={styles.link} onClick={() => setSort(sort === "order" ? "rank" : "order")}>
          {sort === "order" ? tr("按出现顺序") : tr("难的在前")}
        </button>
      </div>

      {group === "new" && shown.length > 0 && (
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
          <button type="button" className={styles.primarySmall} disabled={pickedNew.length === 0} onClick={() => void savePicked(pickedNew)}>
            {tr("加入生词本（{n}）", { n: pickedNew.length })}
          </button>
          <button type="button" className={styles.ghost} disabled={pickedNew.length === 0} onClick={() => void markKnown(pickedNew.map((w) => w.lemma), true)}>
            {tr("标为认识")}
          </button>
        </div>
      )}

      {shown.length === 0 ? (
        <p className={styles.empty}>{group === "new" ? tr("没有超过词汇量的生词。") : tr("这一组是空的。")}</p>
      ) : (
        <ul className={styles.list}>
          {shown.map((w) => {
            const g = glosses.get(w.lemma);
            return (
              <li key={w.lemma} className={styles.row}>
                {group === "new" && (
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
                    <button type="button" className={styles.word} title={tr("在文中找到它")} onClick={() => onLocate(w.lemma)}>
                      {w.lemma}
                    </button>
                    {g?.pos && <span className={styles.pos}>{g.pos}</span>}
                    {w.count > 1 && <span className={styles.faint}>×{w.count}</span>}
                  </div>
                  <div className={styles.gloss}>{g ? (g.gloss ?? "—") : "…"}</div>
                </div>
                <span className={styles.rank} title={tr("词频排名")}>
                  {w.rank ? `#${w.rank}` : tr("罕见")}
                </span>
                <button type="button" className={styles.link} onClick={() => onLookup(w.lemma)}>
                  {tr("查")}
                </button>
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
    </div>
  );
}
