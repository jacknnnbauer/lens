// 生词本（SPEC M4、画板 04）：收藏的词、语境句与来源、标签、熟悉度、语境卡。
// 复习调度（今日待复习、评分）在 M8 接 FSRS。

import { openUrl } from "@tauri-apps/plugin-opener";
import { useCallback, useEffect, useState } from "react";
import type { ContextRow } from "../../ipc/history";
import {
  deleteWord,
  listTags,
  listWords,
  saveWord,
  unlinkWordContext,
  updateWord,
  wordbookStats,
  type TagCount,
  type Word,
  type WordbookStats,
  type WordQuery,
  type WordStatus,
} from "../../ipc/words";
import { toast } from "../../stores/toast";
import styles from "./WordbookPage.module.css";

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

function Sentence({ ctx }: { ctx: ContextRow }) {
  return (
    <>
      {ctx.sentence.slice(0, ctx.hlStart)}
      <mark>{ctx.sentence.slice(ctx.hlStart, ctx.hlEnd)}</mark>
      {ctx.sentence.slice(ctx.hlEnd)}
    </>
  );
}

function BackLink({ url }: { url: string | null }) {
  if (!url || !/^https?:\/\//i.test(url)) return null;
  return (
    <a
      href={url}
      title={url}
      onClick={(e) => {
        e.preventDefault();
        void openUrl(url);
      }}
    >
      回到原处 ↗
    </a>
  );
}

function Familiarity({ value, onChange, word }: { value: number; onChange: (v: number) => void; word: string }) {
  return (
    <span role="group" aria-label={`「${word}」熟悉度 ${value}/5`} className={styles.dots}>
      {[1, 2, 3, 4, 5].map((n) => (
        <button
          key={n}
          type="button"
          className={styles.dot}
          data-on={n <= value || undefined}
          aria-label={`熟悉度设为 ${n}`}
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
    toast(`已更新「${word.lemma}」`);
    onChanged();
  };

  return (
    <div className={styles.detail}>
      <div className={styles.fields}>
        <label>
          <span>简释</span>
          <input value={brief} onChange={(e) => setBrief(e.target.value)} placeholder="一句话释义" />
        </label>
        <label>
          <span>标签</span>
          <input value={tags} onChange={(e) => setTags(e.target.value)} placeholder="用逗号分隔，如：论文，技术文档" />
        </label>
        <div className={styles.fieldActions}>
          <button type="button" className={styles.primary} onClick={() => void saveEdits()}>
            保存
          </button>
          <button type="button" className={styles.ghost} onClick={() => onLookup(word.lemma)}>
            查词
          </button>
          {confirmDelete ? (
            <>
              <button
                type="button"
                className={styles.danger}
                onClick={() => void deleteWord(word.id).then(() => (toast(`已从生词本移除「${word.lemma}」`), onChanged()))}
              >
                确认移除
              </button>
              <button type="button" className={styles.ghost} onClick={() => setConfirmDelete(false)}>
                取消
              </button>
            </>
          ) : (
            <button type="button" className={styles.ghost} onClick={() => setConfirmDelete(true)}>
              移出生词本
            </button>
          )}
        </div>
      </div>

      {word.contexts.length > 0 && (
        <div className={styles.contexts}>
          <div className={styles.label}>同一个词，{word.contexts.length} 个语境</div>
          <ul>
            {word.contexts.map((c) => (
              <li key={c.id}>
                <p className={styles.quote}>
                  “<Sentence ctx={c} />”
                </p>
                <div className={styles.meta}>
                  <span>{[c.app, c.windowTitle].filter(Boolean).join(" · ")}</span>
                  <span>{new Date(c.createdAt).toLocaleDateString()}</span>
                  <BackLink url={c.url} />
                  <button
                    type="button"
                    className={styles.linkBtn}
                    onClick={() => void unlinkWordContext(word.id, c.id).then(onChanged)}
                    aria-label="移除这条语境"
                  >
                    移除
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

/** 语境卡：正面是当时读到的句子，Space / 点击翻到背面看释义。 */
function ContextCard({ word }: { word: Word }) {
  const [flipped, setFlipped] = useState(false);
  const ctx = word.contexts[0];
  return (
    <button
      type="button"
      className={styles.card}
      aria-pressed={flipped}
      aria-label={flipped ? `${word.lemma}：${word.brief ?? ""}` : "语境卡，按 Space 翻面"}
      onClick={() => setFlipped((f) => !f)}
    >
      {!flipped ? (
        <>
          <span className={styles.cardSide}>正面 · 你当时读到的句子</span>
          {ctx ? (
            <span className={styles.cardQuote}>
              “<Sentence ctx={ctx} />”
            </span>
          ) : (
            <span className={styles.cardWord}>{word.lemma}</span>
          )}
          {ctx && <span className={styles.cardSource}>{[ctx.app, ctx.windowTitle].filter(Boolean).join(" · ")}</span>}
        </>
      ) : (
        <>
          <span className={styles.cardSide}>背面</span>
          <span className={styles.cardWord}>{word.lemma}</span>
          <span className={styles.cardBrief}>{word.brief ?? "（没有简释，可在列表里补上）"}</span>
        </>
      )}
    </button>
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

  const reload = useCallback(async () => {
    try {
      const [w, t, s] = await Promise.all([listWords({ ...query, search, limit: 500 }), listTags(), wordbookStats()]);
      setWords(w);
      setTags(t);
      setStats(s);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, [query, search]);

  useEffect(() => {
    if (!active) return;
    const t = setTimeout(() => void reload(), search ? SEARCH_DEBOUNCE_MS : 0);
    return () => clearTimeout(t);
  }, [active, reload, search]);

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
    toast(`已收藏「${lemma}」`);
    void reload();
  };

  return (
    <div className={styles.column}>
      <header className={styles.head}>
        <h1 className={styles.title}>生词本</h1>
        <div role="group" aria-label="视图" className={styles.segmented}>
          <button type="button" aria-pressed={view === "list"} onClick={() => setView("list")}>
            列表
          </button>
          <button type="button" aria-pressed={view === "cards"} onClick={() => setView("cards")}>
            语境卡
          </button>
        </div>
        <label className={styles.search}>
          <span className="sr-only">搜索生词</span>
          <input type="search" placeholder="搜索词或释义" value={search} onChange={(e) => setSearch(e.target.value)} spellCheck={false} />
        </label>
      </header>

      {stats && (
        <div className={styles.stats}>
          <div className={styles.stat}>
            <span>已收藏</span>
            <strong>{stats.total}</strong>
          </div>
          <div className={styles.stat}>
            <span>本周在阅读中再遇</span>
            <strong>
              {stats.metThisWeek}
              <small> 次</small>
            </strong>
          </div>
          <div className={styles.stat}>
            <span>查过 3 次以上未收藏</span>
            <strong>
              {stats.frequentUnsaved.length}
              {stats.frequentUnsaved.length > 0 && (
                <button type="button" className={styles.linkBtn} aria-expanded={showFrequent} onClick={() => setShowFrequent((s) => !s)}>
                  {showFrequent ? "收起" : "查看"}
                </button>
              )}
            </strong>
          </div>
        </div>
      )}

      {showFrequent && stats && stats.frequentUnsaved.length > 0 && (
        <ul className={styles.frequent} aria-label="常查未收藏">
          {stats.frequentUnsaved.map((w) => (
            <li key={w}>
              <button type="button" className={styles.wordBtn} onClick={() => onLookup(w)}>
                {w}
              </button>
              <button type="button" className={styles.ghost} onClick={() => void saveFrequent(w)}>
                收藏
              </button>
            </li>
          ))}
        </ul>
      )}

      <div className={styles.filters}>
        <div role="group" aria-label="状态" className={styles.segmented}>
          {STATUSES.map((s) => (
            <button key={s.name} type="button" aria-pressed={query.status === s.id} onClick={() => setQuery((q) => ({ ...q, status: s.id }))}>
              {s.name}
            </button>
          ))}
        </div>
        <label className={styles.sort}>
          排序
          <select value={query.sort} onChange={(e) => setQuery((q) => ({ ...q, sort: e.target.value as WordQuery["sort"] }))}>
            <option value="added">最近收藏</option>
            <option value="alpha">字母</option>
            <option value="familiarity">熟悉度</option>
          </select>
        </label>
      </div>

      {tags.length > 0 && (
        <div role="group" aria-label="标签" className={styles.tags}>
          <button type="button" aria-pressed={!query.tag} onClick={() => setQuery((q) => ({ ...q, tag: undefined }))}>
            全部
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
          {stats?.total ? "没有符合条件的词。" : "生词本还是空的。查词时按 Ctrl+S（或点星标）收藏，划词收藏会带上原句。"}
        </p>
      ) : view === "cards" ? (
        <div className={styles.cardGrid}>
          {words.map((w) => (
            <ContextCard key={w.id} word={w} />
          ))}
        </div>
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
                    <p className={styles.quote}>
                      “<Sentence ctx={ctx} />”
                    </p>
                  )}
                  <div className={styles.meta}>
                    {ctx && <span>{[ctx.app, ctx.windowTitle].filter(Boolean).join(" · ")}</span>}
                    {w.tags.map((t) => (
                      <span key={t}># {t}</span>
                    ))}
                    {w.lookups > 0 && <span>查过 {w.lookups} 次</span>}
                    {w.contexts.length > 1 && <span>{w.contexts.length} 个语境</span>}
                    {ctx && <BackLink url={ctx.url} />}
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
                    {open === w.id ? "收起" : "详情"}
                  </button>
                </div>
                {open === w.id && <WordDetail word={w} onChanged={() => void reload()} onLookup={onLookup} />}
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
