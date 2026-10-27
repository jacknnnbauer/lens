// 生词本（SPEC M4、画板 04）：收藏的词、语境句与来源、标签、熟悉度、语境卡。
// 复习（M8）：「开始复习」进入 ReviewSession，调度由 Rust 的 FSRS 完成。

import { SourceLink, sourceText } from "../../components/SourceLink";
import { CardWall } from "../../components/CardWall";
import { useCallback, useEffect, useState } from "react";
import {
  deleteWord,
  listTags,
  listWords,
  reviewCounts,
  saveWord,
  unlinkWordContext,
  updateWord,
  wordbookStats,
  type ReviewCounts,
  type TagCount,
  type Word,
  type WordbookStats,
  type WordQuery,
  type WordStatus,
} from "../../ipc/words";
import { useBatch } from "../../stores/batch";
import { toast } from "../../stores/toast";
import { formatInterval, ReviewSession, startOfToday } from "./ReviewSession";
import { Sentence } from "./Sentence";
import { AnkiExportDialog } from "./AnkiExportDialog";
import { CardDefinition } from "../../components/CardDefinition";
import { useDicts } from "../../stores/dicts";
import { useSettings } from "../../stores/settings";

/** 语境卡背面用哪本词典：自动，或指定一本本地词典 */
function CardDictSelect() {
  const dicts = useDicts((s) => s.dicts).filter((d) => d.kind !== "online" && d.loaded);
  const value = useSettings((s) => s.settings?.wordbook?.cardDict ?? "");
  // 立即写回：背面释义由 Rust 按设置挑词典
  const updateNow = useSettings((s) => s.updateNow);
  return (
    <label className={styles.sort}>
      <span>{tr("背面释义")}</span>
      <select value={dicts.some((d) => d.id === value) ? value : ""} onChange={(e) => void updateNow({ wordbook: { cardDict: e.target.value } })}>
        <option value="">{tr("自动（跳过搭配、词频类词典）")}</option>
        {dicts.map((d) => (
          <option key={d.id} value={d.id}>
            {d.name}
          </option>
        ))}
      </select>
    </label>
  );
}
import styles from "./WordbookPage.module.css";
import { tr } from "../../i18n";

const SEARCH_DEBOUNCE_MS = 200;
const STATUSES: Array<{ id: WordStatus | undefined; name: string }> = [
  { id: undefined, name: "全部" },
  { id: "new", name: "新词" },
  { id: "learning", name: "学习中" },
  { id: "mastered", name: "已掌握" },
];

interface Props {
  onLookup: (word: string) => void;
  active: boolean;
}

function Familiarity({ value, onChange, word }: { value: number; onChange: (v: number) => void; word: string }) {
  return (
    <span role="group" aria-label={tr("「{word}」熟悉度 {v}/5", { word, v: value })} className={styles.dots}>
      {[1, 2, 3, 4, 5].map((n) => (
        <button
          key={n}
          type="button"
          className={styles.dot}
          data-on={n <= value || undefined}
          aria-label={tr("熟悉度设为 {n}", { n })}
          aria-pressed={n === value}
          // 再点当前值归零（回到新词）
          onClick={() => onChange(n === value ? 0 : n)}
        />
      ))}
    </span>
  );
}

function WordDetail({ word, onChanged, onLookup }: { word: Word; onChanged: () => void; onLookup: (w: string) => void }) {
  const [brief, setBrief] = useState(word.brief ?? "");
  const [tags, setTags] = useState(word.tags.join("，"));
  const [confirmDelete, setConfirmDelete] = useState(false);

  const saveEdits = async () => {
    await updateWord(word.id, { brief, tags: tags.split(/[,，\s]+/).filter(Boolean) });
    toast(tr("已更新「{word}」", { word: word.lemma }));
    onChanged();
  };

  return (
    <div className={styles.detail}>
      <div className={styles.fields}>
        <label>
          <span>{tr("简释")}</span>
          <input value={brief} onChange={(e) => setBrief(e.target.value)} placeholder={tr("一句话释义")} />
        </label>
        <label>
          <span>{tr("标签")}</span>
          <input value={tags} onChange={(e) => setTags(e.target.value)} placeholder={tr("用逗号分隔，如：论文，技术文档")} />
        </label>
        <div className={styles.fieldActions}>
          <button type="button" className={styles.primary} onClick={() => void saveEdits()}>
            {tr("保存")}
          </button>
          <button type="button" className={styles.ghost} onClick={() => onLookup(word.lemma)}>
            {tr("查词")}
          </button>
          {confirmDelete ? (
            <>
              <button
                type="button"
                className={styles.danger}
                onClick={() => void deleteWord(word.id).then(() => (toast(tr("已从生词本移除「{word}」", { word: word.lemma })), onChanged()))}
              >
                {tr("确认移除")}
              </button>
              <button type="button" className={styles.ghost} onClick={() => setConfirmDelete(false)}>
                {tr("取消")}
              </button>
            </>
          ) : (
            <button type="button" className={styles.ghost} onClick={() => setConfirmDelete(true)}>
              {tr("移出生词本")}
            </button>
          )}
        </div>
      </div>

      {word.contexts.length > 0 && (
        <div className={styles.contexts}>
          <div className={styles.label}>{tr("同一个词，{n} 个语境", { n: word.contexts.length })}</div>
          <ul>
            {word.contexts.map((c) => (
              <li key={c.id}>
                <p className={styles.quote} data-word-lookup="">
                  “<Sentence ctx={c} />”
                </p>
                <div className={styles.meta}>
                  <SourceLink src={c} />
                  <span>{new Date(c.createdAt).toLocaleDateString()}</span>
                  <button
                    type="button"
                    className={styles.linkBtn}
                    onClick={() => void unlinkWordContext(word.id, c.id).then(onChanged)}
                    aria-label={tr("移除这条语境")}
                  >
                    {tr("移除")}
                  </button>
                </div>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

/** 语境卡：正面是当时读到的句子，Space / 点击翻到背面看原词、简释和词典释义。 */
function ContextCard({ word, onLookup }: { word: Word; onLookup: (w: string) => void }) {
  const [flipped, setFlipped] = useState(false);
  const ctx = word.contexts[0];
  return (
    <div className={styles.card} data-flipped={flipped || undefined} data-ui="card">
    <button
      type="button"
      className={styles.cardFlip}
      aria-pressed={flipped}
      aria-label={flipped ? `${word.lemma}: ${word.brief ?? ""}` : tr("语境卡，按 Space 翻面")}
      onClick={() => setFlipped((f) => !f)}
    >
      {!flipped ? (
        <>
          {ctx ? (
            <span className={styles.cardQuote}>
              “<Sentence ctx={ctx} />”
            </span>
          ) : (
            <span className={styles.cardWord}>{word.lemma}</span>
          )}
          {ctx && <span className={styles.cardSource}>{sourceText(ctx)}</span>}
        </>
      ) : (
        <>
          <span className={styles.cardWord}>{word.lemma}</span>
          {ctx && (
            <span className={styles.cardBackQuote}>
              “<Sentence ctx={ctx} />”
            </span>
          )}
        </>
      )}
    </button>
      {flipped && <CardDefinition word={word.lemma} onLookup={onLookup} />}
    </div>
  );
}

export function WordbookPage({ onLookup, active }: Props) {
  const [words, setWords] = useState<Word[]>([]);
  const [tags, setTags] = useState<TagCount[]>([]);
  const [stats, setStats] = useState<WordbookStats | null>(null);
  const [query, setQuery] = useState<WordQuery>({ sort: "added" });
  const [search, setSearch] = useState("");
  const [view, setView] = useState<"list" | "cards">("list");
  const [open, setOpen] = useState<number | null>(null);
  const [showFrequent, setShowFrequent] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [counts, setCounts] = useState<ReviewCounts | null>(null);
  const [reviewing, setReviewing] = useState(false);
  const [exportOpen, setExportOpen] = useState(false);

  const reload = useCallback(async () => {
    try {
      const [w, t, s, rc] = await Promise.all([listWords({ ...query, search, limit: 500 }), listTags(), wordbookStats(), reviewCounts(startOfToday())]);
      setWords(w);
      setTags(t);
      setStats(s);
      setCounts(rc);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, [query, search]);

  useEffect(() => {
    if (!active || reviewing) return;
    const t = setTimeout(() => void reload(), search ? SEARCH_DEBOUNCE_MS : 0);
    return () => clearTimeout(t);
  }, [active, reload, search, reviewing]);

  const setFamiliarity = async (w: Word, v: number) => {
    setWords((prev) => prev.map((x) => (x.id === w.id ? { ...x, familiarity: v } : x)));
    try {
      await updateWord(w.id, { familiarity: v });
    } catch (e) {
      setError(String(e));
    }
  };

  const saveFrequent = async (lemma: string) => {
    await saveWord(lemma);
    toast(tr("已收藏「{word}」", { word: lemma }));
    void reload();
  };

  if (reviewing) {
    return (
      <div className={styles.column}>
        <ReviewSession onExit={() => setReviewing(false)} onLookup={onLookup} />
      </div>
    );
  }

  const toReview = counts ? counts.due + counts.new : 0;

  return (
    <div className={styles.column}>
      <header className={styles.head}>
        <h1 className={styles.title}>{tr("生词本")}</h1>
        <div role="group" aria-label={tr("视图")} className={styles.segmented}>
          <button type="button" aria-pressed={view === "list"} onClick={() => setView("list")}>
            {tr("列表")}
          </button>
          <button type="button" aria-pressed={view === "cards"} onClick={() => setView("cards")}>
            {tr("语境卡")}
          </button>
        </div>
        <label className={styles.search}>
          <span className="sr-only">{tr("搜索生词")}</span>
          <input type="search" placeholder={tr("搜索词或释义")} value={search} onChange={(e) => setSearch(e.target.value)} spellCheck={false} />
        </label>
        <button type="button" className={styles.ghost} onClick={() => useBatch.getState().openDialog("wordbook")}>
          {tr("批量导出…")}
        </button>
        <button type="button" className={styles.ghost} title={tr("按当前筛选导出：Anki 牌组包（带发音和卡片模板）、Anki 文本或 CSV")} onClick={() => setExportOpen(true)}>
          {tr("导出到 Anki…")}
        </button>
        <button type="button" className={styles.reviewBtn} disabled={toReview === 0} onClick={() => setReviewing(true)}>
          {tr("开始复习 · {n}", { n: toReview })}
        </button>
      </header>

      {stats && (
        <div className={styles.stats}>
          <div className={styles.stat}>
            <span>{tr("已收藏")}</span>
            <strong>{stats.total}</strong>
          </div>
          {counts && (
            <div className={styles.stat}>
              <span>{tr("今日待复习")}</span>
              <strong>
                {counts.due}
                <small>{tr("+ {n} 个新词", { n: counts.new })}</small>
                {counts.doneToday > 0 && <small>{tr("今天已复习 {n} 次", { n: counts.doneToday })}</small>}
              </strong>
            </div>
          )}
          <div className={styles.stat}>
            <span>{tr("本周在阅读中再遇")}</span>
            <strong>
              {stats.metThisWeek}
              <small> {tr("次")}</small>
            </strong>
          </div>
          <div className={styles.stat}>
            <span>{tr("查过 3 次以上未收藏")}</span>
            <strong>
              {stats.frequentUnsaved.length}
              {stats.frequentUnsaved.length > 0 && (
                <button type="button" className={styles.linkBtn} aria-expanded={showFrequent} onClick={() => setShowFrequent((s) => !s)}>
                  {tr(showFrequent ? "收起" : "查看")}
                </button>
              )}
            </strong>
          </div>
        </div>
      )}

      {showFrequent && stats && stats.frequentUnsaved.length > 0 && (
        <ul className={styles.frequent} aria-label={tr("常查未收藏")}>
          {stats.frequentUnsaved.map((w) => (
            <li key={w}>
              <button type="button" className={styles.wordBtn} onClick={() => onLookup(w)}>
                {w}
              </button>
              <button type="button" className={styles.ghost} onClick={() => void saveFrequent(w)}>
                {tr("收藏")}
              </button>
            </li>
          ))}
        </ul>
      )}

      <div className={styles.filters}>
        <div role="group" aria-label={tr("状态")} className={styles.segmented}>
          {STATUSES.map((s) => (
            <button key={s.name} type="button" aria-pressed={query.status === s.id} onClick={() => setQuery((q) => ({ ...q, status: s.id }))}>
              {tr(s.name)}
            </button>
          ))}
        </div>
        {view === "cards" && <CardDictSelect />}
        <label className={styles.sort} data-plain={view === "cards" || undefined}>
          {tr("排序")}
          <select value={query.sort} onChange={(e) => setQuery((q) => ({ ...q, sort: e.target.value as WordQuery["sort"] }))}>
            <option value="added">{tr("最近收藏")}</option>
            <option value="alpha">{tr("字母")}</option>
            <option value="familiarity">{tr("熟悉度")}</option>
            <option value="due">{tr("下次复习")}</option>
          </select>
        </label>
      </div>

      {tags.length > 0 && (
        <div role="group" aria-label={tr("标签")} className={styles.tags}>
          <button type="button" aria-pressed={!query.tag} onClick={() => setQuery((q) => ({ ...q, tag: undefined }))}>
            {tr("全部")}
          </button>
          {tags.map((t) => (
            <button key={t.name} type="button" aria-pressed={query.tag === t.name} onClick={() => setQuery((q) => ({ ...q, tag: t.name }))}>
              # {t.name}
              <span>{t.count}</span>
            </button>
          ))}
        </div>
      )}

      {error && (
        <p role="alert" className={styles.error}>
          {error}
        </p>
      )}

      {words.length === 0 ? (
        <p className={styles.empty}>
          {tr(stats?.total ? "没有符合条件的词。" : "生词本还是空的。查词时按 Ctrl+S（或点星标）收藏，划词收藏会带上原句。")}
        </p>
      ) : view === "cards" ? (
        // 语境卡用卡片墙：同一列上下紧挨，翻到背面变长也不留空
        <CardWall minColumn={260} gap={12} items={words.map((w) => ({ key: String(w.id), node: <ContextCard word={w} onLookup={onLookup} /> }))} />
      ) : (
        <ul className={styles.list}>
          {words.map((w) => {
            const ctx = w.contexts[0];
            return (
              <li key={w.id} className={styles.row}>
                <div className={styles.rowMain}>
                  <div className={styles.line}>
                    <button type="button" className={styles.wordBtn} onClick={() => onLookup(w.lemma)}>
                      {w.lemma}
                    </button>
                    {w.brief && <span className={styles.brief}>{w.brief}</span>}
                  </div>
                  {ctx && (
                    <p className={styles.quote} data-word-lookup="">
                      “<Sentence ctx={ctx} />”
                    </p>
                  )}
                  <div className={styles.meta}>
                    {ctx && <SourceLink src={ctx} />}
                    {w.tags.map((t) => (
                      <span key={t}># {t}</span>
                    ))}
                    {w.lookups > 0 && <span>{tr("查过 {n} 次", { n: w.lookups })}</span>}
                    {w.contexts.length > 1 && <span>{tr("{n} 个语境", { n: w.contexts.length })}</span>}
                    {w.dueAt !== null && <span>{w.dueAt <= Date.now() ? tr("该复习了") : tr("{t}后复习", { t: formatInterval(w.dueAt - Date.now()) })}</span>}
                  </div>
                </div>
                <div className={styles.side}>
                  <Familiarity value={w.familiarity} word={w.lemma} onChange={(v) => void setFamiliarity(w, v)} />
                  <button
                    type="button"
                    className={styles.linkBtn}
                    aria-expanded={open === w.id}
                    onClick={() => setOpen((o) => (o === w.id ? null : w.id))}
                  >
                    {tr(open === w.id ? "收起" : "详情")}
                  </button>
                </div>
                {open === w.id && <WordDetail word={w} onChanged={() => void reload()} onLookup={onLookup} />}
              </li>
            );
          })}
        </ul>
      )}
      <AnkiExportDialog open={exportOpen} onClose={() => setExportOpen(false)} query={{ status: query.status, tag: query.tag, search }} />
    </div>
  );
}
