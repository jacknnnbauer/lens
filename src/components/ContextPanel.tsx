// 右侧栏（三栏布局）：当前词的 COCA 词频与搭配、AI 双语例句，以及本次查询的详情（各词典命中情况与耗时）。

import { useLookup } from "../stores/lookup";
import { briefOf } from "../stores/save";
import { WordInsights } from "./WordInsights";
import styles from "./ContextPanel.module.css";
import { tr } from "../i18n";

const STATUS_LABEL = {
  pending: "查询中…",
  hit: "命中",
  miss: "未收录",
  error: "出错",
  manual: "待手动查询",
  web: "网页",
  skipped: "本地已命中，未查",
} as const;

export function ContextPanel() {
  const handle = useLookup((s) => s.handle);
  const cards = useLookup((s) => s.cards);
  const done = useLookup((s) => s.done);

  if (!handle) {
    return (
      <aside aria-label={tr("查询详情")} className={styles.aside}>
        <p className={styles.empty}>{tr("查询后，这里显示各词典的命中情况与耗时。")}</p>
      </aside>
    );
  }
  const hits = cards.filter((c) => c.status === "hit").length;
  const headword = cards.find((c) => c.status === "hit")?.entries[0]?.headword ?? "";
  const word = (headword || handle.normalized || handle.query).trim();

  return (
    <aside aria-label={tr("查询详情")} className={styles.aside}>
      {word && (done || hits > 0) && <WordInsights key={word.toLowerCase()} word={word} brief={briefOf(cards, word)} />}

      <section className={styles.section}>
        <div className={styles.label}>{tr("查询")}</div>
        <dl className={styles.kv}>
          <dt>{tr("输入")}</dt>
          <dd>{handle.query}</dd>
          {handle.normalized !== handle.query.trim() && (
            <>
              <dt>{tr("查询键")}</dt>
              <dd>{handle.normalized}</dd>
            </>
          )}
        </dl>
      </section>

      <section className={styles.section}>
        <div className={styles.label}>
          {tr("词典 · {hits}/{total} 命中", { hits, total: cards.length })}
        </div>
        <ul className={styles.list}>
          {cards.map((c) => (
            <li key={c.id} className={styles.row} data-status={c.status}>
              <span className={styles.name}>{c.name}</span>
              <span className={styles.status}>
                {c.status === "hit" && c.entries.length > 1 ? tr("{n} 条", { n: c.entries.length }) : tr(STATUS_LABEL[c.status])}
                {c.ms != null && ` · ${c.ms < 10 ? c.ms.toFixed(1) : Math.round(c.ms)} ms`}
              </span>
            </li>
          ))}
        </ul>
      </section>
    </aside>
  );
}
