// 查词页的反查条：查的是中文时，在词典卡片上方列出释义里有这段中文的英文词（全文索引），点「全部」到反查页。

import { useEffect, useState } from "react";
import { hasCjk, reverseLookup, type ReverseResult } from "../ipc/fulltext";
import { useSearchPage } from "../stores/searchPage";
import styles from "./ReverseStrip.module.css";
import { tr } from "../i18n";

const SHOWN = 10;

interface Props {
  query: string;
  onLookup: (word: string) => void;
  onOpenSearch: () => void;
}

export function ReverseStrip({ query, onLookup, onOpenSearch }: Props) {
  const [r, setR] = useState<(ReverseResult & { q: string }) | null>(null);
  const q = query.trim();
  const cjk = hasCjk(q);

  useEffect(() => {
    if (!cjk) return;
    let alive = true;
    const t = setTimeout(() => {
      reverseLookup(q, SHOWN + 1).then(
        (res) => alive && setR({ ...res, q }),
        () => alive && setR(null),
      );
    }, 300);
    return () => {
      alive = false;
      clearTimeout(t);
    };
  }, [q, cjk]);

  if (!cjk || !r || r.q !== q) return null;

  const openAll = () => {
    useSearchPage.getState().setQuery(q);
    useSearchPage.getState().setMode("reverse");
    onOpenSearch();
  };

  if (r.indexed === 0) {
    return (
      <p className={styles.hint}>
        {tr("想用中文找英文词？给双解词典建立全文索引后就能反查。")}{" "}
        <button type="button" className={styles.link} onClick={openAll}>
          {tr("去建立")}
        </button>
      </p>
    );
  }
  if (r.words.length === 0) return null;

  return (
    <section className={styles.strip} aria-label={tr("反查结果")} data-ui="card">
      <span className={styles.label}>{tr("反查「{q}」", { q })}</span>
      <ul className={styles.words}>
        {r.words.slice(0, SHOWN).map((w) => (
          <li key={w.word}>
            <button type="button" className={styles.chip} data-ui="chip" title={`${w.pos ?? ""} ${w.gloss}`.trim()} onClick={() => onLookup(w.word)}>
              <span className={styles.word}>{w.word}</span>
              <span className={styles.gloss}>{w.gloss}</span>
            </button>
          </li>
        ))}
      </ul>
      <button type="button" className={styles.link} onClick={openAll}>
        {r.words.length > SHOWN ? tr("全部 →") : tr("详细 →")}
      </button>
    </section>
  );
}
