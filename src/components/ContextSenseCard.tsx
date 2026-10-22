// 「当前语境」：这句话里，这个词是词典里的哪个意思、为什么（本地判断：词性 + 搭配，见 Rust context_sense）。
// 句子来自划词 / 窗口内查词捕获的语境句；也可以自己粘贴一句话，再点其中的词。

import { useEffect, useState } from "react";
import { tr } from "../i18n";
import { contextSense, type ContextSenseResult } from "../ipc/lookup";
import type { Card } from "../stores/lookup";
import styles from "./ContextSenseCard.module.css";

export interface SentenceCtx {
  sentence: string;
  /** 所查的词在句中的 UTF-16 区间 */
  hlStart: number;
  hlEnd: number;
}

interface Props {
  /** 词典里的词头（原形） */
  word: string;
  cards: Card[];
  ctx: SentenceCtx | null;
  /** 粘贴了一句话并选了其中的词：查这个词，并用这句话判断 */
  onPick: (word: string, ctx: SentenceCtx) => void;
  /** 句子已经在别处显示（查词小窗底部的语境栏）时不再重复 */
  hideSentence?: boolean;
}

const POS_NAME: Record<string, string> = { "n.": "名词", "v.": "动词", "adj.": "形容词", "adv.": "副词" };

/** 句子按词切开，带 UTF-16 区间 */
function splitWords(s: string): Array<{ text: string; start: number; end: number }> {
  const out: Array<{ text: string; start: number; end: number }> = [];
  const re = /[A-Za-z][A-Za-z'’-]*/g;
  for (let m = re.exec(s); m; m = re.exec(s)) out.push({ text: m[0], start: m.index, end: m.index + m[0].length });
  return out;
}

export function ContextSenseCard({ word, cards, ctx, onPick, hideSentence }: Props) {
  const [result, setResult] = useState<ContextSenseResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pasting, setPasting] = useState(false);
  const [draft, setDraft] = useState("");
  const [showWhy, setShowWhy] = useState(false);
  const hits = cards.filter((c) => c.status === "hit");
  const hitKey = hits.map((c) => c.id).join(",");

  useEffect(() => {
    setResult(null);
    setError(null);
    if (!ctx || !word || hits.length === 0) return;
    let alive = true;
    contextSense({
      word,
      sentence: ctx.sentence,
      hlStart: ctx.hlStart,
      hlEnd: ctx.hlEnd,
      dicts: hits.map((c) => ({ id: c.id, name: c.name, entries: c.entries.map((e) => e.html) })),
    }).then(
      (r) => alive && setResult(r),
      (e) => alive && setError(String(e)),
    );
    return () => {
      alive = false;
    };
    // hits 由 hitKey 代表：卡片内容变了（查询完成）才重算
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [word, ctx?.sentence, ctx?.hlStart, ctx?.hlEnd, hitKey]);

  const draftWords = splitWords(draft.trim());

  // 没有语境句：只给一个入口，粘贴整句
  if (!ctx && !pasting) {
    return (
      <div className={styles.entry}>
        <button type="button" className={styles.link} onClick={() => setPasting(true)}>
          {tr("粘贴一句话，看这个词在句中是什么意思")}
        </button>
      </div>
    );
  }

  return (
    <section className={styles.card} aria-label={tr("当前语境")} data-word-lookup="">
      <header className={styles.head}>
        <span className={styles.title}>{tr("当前语境")}</span>
        {result?.best && <span className={styles.badge} data-sure={result.confident || undefined}>{tr(result.confident ? "有把握" : "初步判断")}</span>}
        <button type="button" className={styles.link} onClick={() => setPasting((p) => !p)}>
          {tr(pasting ? "收起" : "换一句")}
        </button>
      </header>

      {pasting && (
        <div className={styles.paste}>
          <textarea
            value={draft}
            rows={2}
            placeholder={tr("粘贴一句英文，例如：The company addressed the issue.")}
            spellCheck={false}
            onChange={(e) => setDraft(e.target.value)}
          />
          {draftWords.length > 0 && (
            <div className={styles.words} role="group" aria-label={tr("点句中要理解的词")}>
              <span>{tr("点要理解的词：")}</span>
              {draftWords.map((w, i) => (
                <button
                  key={`${w.start}-${i}`}
                  type="button"
                  onClick={() => {
                    onPick(w.text, { sentence: draft.trim(), hlStart: w.start, hlEnd: w.end });
                    setPasting(false);
                  }}
                >
                  {w.text}
                </button>
              ))}
            </div>
          )}
        </div>
      )}

      {ctx && !hideSentence && (
        <p className={styles.sentence}>
          {ctx.sentence.slice(0, ctx.hlStart)}
          <mark>{ctx.sentence.slice(ctx.hlStart, ctx.hlEnd)}</mark>
          {ctx.sentence.slice(ctx.hlEnd)}
        </p>
      )}

      {ctx && error && <p className={styles.note}>{error}</p>}
      {ctx && !result && !error && hits.length > 0 && <p className={styles.note}>{tr("判断中…")}</p>}
      {ctx && result && !result.best && <p className={styles.note}>{tr("词典里没有抽出可比较的义项")}</p>}

      {ctx && result?.best && (
        <>
          <p className={styles.sense}>
            <span className={styles.k}>{tr("当前词义")}</span>
            <span className={styles.v}>{result.best.label}</span>
          </p>
          <p className={styles.meta}>
            {result.pos && (
              <span>
                {tr("词性：{pos}", { pos: tr(POS_NAME[result.pos] ?? result.pos) })}
              </span>
            )}
            <span>{tr("{dict} · 义项 {n}", { dict: result.best.firstDict, n: result.best.senseNo })}</span>
            {result.best.dicts.length > 1 && <span>{tr("{n} 本词典都有这个意思", { n: result.best.dicts.length })}</span>}
          </p>
          {result.reasons.length > 0 && (
            <ul className={styles.reasons}>
              {result.reasons.map((r) => (
                <li key={r}>{r}</li>
              ))}
            </ul>
          )}
          {(result.ruledOut.length > 0 || result.others.length > 0) && (
            <>
              <button type="button" className={styles.link} aria-expanded={showWhy} onClick={() => setShowWhy((s) => !s)}>
                {tr(showWhy ? "收起" : "为什么不是其他意思？")}
              </button>
              {showWhy && (
                <ul className={styles.reasons} data-muted="">
                  {result.ruledOut.map((r) => (
                    <li key={r}>{r}</li>
                  ))}
                  {result.others.map((o) => (
                    <li key={o.label}>{tr("备选：{label}（{dict} · 义项 {n}）", { label: o.label, dict: o.firstDict, n: o.senseNo })}</li>
                  ))}
                </ul>
              )}
            </>
          )}
        </>
      )}
    </section>
  );
}
