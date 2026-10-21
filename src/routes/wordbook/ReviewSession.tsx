// 复习（M8、画板 04）：正面是当时读到的句子，Space 翻面看释义，1–4 评分（按钮上写着下次间隔）。
// 调度在 Rust（FSRS）；一轮做完再取一次队列，「忘了」的词 10 分钟内会回来。

import { useCallback, useEffect, useRef, useState } from "react";
import { tr } from "../../i18n";
import { reviewQueue, reviewRate, type ReviewItem } from "../../ipc/words";
import styles from "./ReviewSession.module.css";
import { Sentence } from "./Sentence";
import { CardDefinition } from "../../components/CardDefinition";
import { sourceText } from "../../components/SourceLink";

const RATINGS = [
  { r: 1, name: "忘了" },
  { r: 2, name: "模糊" },
  { r: 3, name: "记得" },
  { r: 4, name: "简单" },
] as const;

export function startOfToday(): number {
  const d = new Date();
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

/** 间隔写成「10 分钟 / 3 天 / 2 个月 / 1.5 年」 */
export function formatInterval(ms: number): string {
  const min = Math.max(1, Math.round(ms / 60000));
  if (min < 60) return tr("{n} 分钟", { n: min });
  const h = Math.round(min / 60);
  if (h < 24) return tr("{n} 小时", { n: h });
  const d = Math.round(h / 24);
  if (d < 31) return tr("{n} 天", { n: d });
  if (d < 365) return tr("{n} 个月", { n: Math.round(d / 30) });
  return tr("{n} 年", { n: Math.round((d / 365) * 10) / 10 });
}

interface Props {
  onExit: () => void;
  onLookup: (word: string) => void;
}

export function ReviewSession({ onExit, onLookup }: Props) {
  const [items, setItems] = useState<ReviewItem[] | null>(null);
  const [index, setIndex] = useState(0);
  const [flipped, setFlipped] = useState(false);
  const [done, setDone] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const fetchedAt = useRef(Date.now());
  const card = useRef<HTMLButtonElement>(null);

  const load = useCallback(async () => {
    try {
      fetchedAt.current = Date.now();
      setItems(await reviewQueue(startOfToday()));
      setIndex(0);
      setFlipped(false);
      setError(null);
    } catch (e) {
      setError(String(e));
      setItems([]);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const item = items?.[index];

  useEffect(() => {
    card.current?.focus();
  }, [item?.word.id, flipped]);

  const rate = useCallback(
    async (r: number) => {
      if (!item || busy) return;
      setBusy(true);
      try {
        await reviewRate(item.word.id, r);
        setDone((n) => n + 1);
        if (items && index + 1 < items.length) {
          setIndex(index + 1);
          setFlipped(false);
        } else {
          await load();
        }
      } catch (e) {
        setError(String(e));
      } finally {
        setBusy(false);
      }
    },
    [item, busy, items, index, load],
  );

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey || e.altKey || e.metaKey) return;
      const t = e.target as HTMLElement | null;
      if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable)) return;
      if (e.key === "Escape") {
        e.preventDefault();
        onExit();
      } else if (e.key === " " || e.key === "Enter") {
        // 焦点在按钮（含卡片本身）上时由按钮自己响应
        if (t?.tagName === "BUTTON") return;
        e.preventDefault();
        setFlipped((f) => !f);
      } else if (flipped && /^[1-4]$/.test(e.key)) {
        e.preventDefault();
        void rate(Number(e.key));
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [flipped, rate, onExit]);

  const remaining = items ? items.length - index : 0;

  return (
    <section className={styles.session} aria-label={tr("复习")}>
      <header className={styles.head}>
        <button type="button" className={styles.exit} onClick={onExit}>
          {tr("← 结束复习")}
        </button>
        <span className={styles.progress} aria-live="polite">
          {tr("已复习 {done} · 剩余 {left}", { done, left: remaining })}
        </span>
      </header>

      {error && (
        <p role="alert" className={styles.error}>
          {error}
        </p>
      )}

      {items === null ? null : !item ? (
        <div className={styles.finished}>
          <p className={styles.finishedTitle}>{done > 0 ? tr("今天的复习完成了") : tr("现在没有要复习的词")}</p>
          <p className={styles.hint}>
            {done > 0 ? tr("本轮复习了 {n} 次。明天再来，或者先去阅读、收藏新词。", { n: done }) : tr("收藏的新词每天最多引入 20 个；到期的词会出现在这里。")}
          </p>
          <button type="button" className={styles.primary} onClick={onExit}>
            {tr("回到生词本")}
          </button>
        </div>
      ) : (
        <>
          <button
            ref={card}
            type="button"
            className={styles.card}
            aria-pressed={flipped}
            aria-label={flipped ? `${item.word.lemma}: ${item.word.brief ?? ""}` : tr("复习卡，按 Space 翻面")}
            onClick={() => setFlipped((f) => !f)}
          >
            {item.word.contexts[0] ? (
              <span className={styles.quote}>
                “<Sentence ctx={item.word.contexts[0]} />”
              </span>
            ) : (
              <span className={styles.bare}>{item.word.lemma}</span>
            )}
            {item.word.contexts[0] && (
              <span className={styles.source}>{sourceText(item.word.contexts[0])}</span>
            )}
            {flipped ? (
              <span className={styles.back}>
                {item.word.contexts[0] && <span className={styles.word}>{item.word.lemma}</span>}
                <span className={styles.brief}>{item.word.brief ?? tr("（没有简释，可在列表里补上）")}</span>
                {item.word.contexts.slice(1, 4).map((c) => (
                  <span key={c.id} className={styles.more}>
                    <Sentence ctx={c} />
                  </span>
                ))}
              </span>
            ) : (
              <span className={styles.flipHint}>{tr("想一想这个词的意思，按 Space 翻面")}</span>
            )}
          </button>

          {flipped ? (
            <div className={styles.ratings} role="group" aria-label={tr("评分")}>
              {RATINGS.map(({ r, name }) => (
                <button key={r} type="button" className={styles.rating} data-rating={r} disabled={busy} onClick={() => void rate(r)}>
                  <span>
                    <kbd>{r}</kbd> {tr(name)}
                  </span>
                  <small>{formatInterval(item.nextDue[r - 1] - fetchedAt.current)}</small>
                </button>
              ))}
            </div>
          ) : (
            <div className={styles.ratings}>
              <button type="button" className={styles.flip} onClick={() => setFlipped(true)}>
                <kbd>Space</kbd> {tr("翻面")}
              </button>
            </div>
          )}

          {flipped && (
            <div className={styles.definition}>
              <CardDefinition word={item.word.lemma} onLookup={onLookup} />
            </div>
          )}
          <div className={styles.tools}>
            <button type="button" className={styles.linkBtn} onClick={() => onLookup(item.word.lemma)}>
              {tr("查词")}
            </button>
          </div>
        </>
      )}
    </section>
  );
}
