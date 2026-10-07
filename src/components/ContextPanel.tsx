// 右侧栏：本次查询的详情（各词典命中情况与耗时）。
// 画板 01 里的语境句、词根、复习数据随 M3/M4 加入这一栏。

import { useLookup } from "../stores/lookup";
import styles from "./ContextPanel.module.css";

const STATUS_LABEL = { pending: "查询中…", hit: "命中", miss: "未收录", error: "出错" } as const;

export function ContextPanel() {
  const handle = useLookup((s) => s.handle);
  const cards = useLookup((s) => s.cards);

  if (!handle) {
    return (
      <aside aria-label="查询详情" className={styles.aside}>
        <p className={styles.empty}>查询后，这里显示各词典的命中情况与耗时。</p>
      </aside>
    );
  }
  const hits = cards.filter((c) => c.status === "hit").length;

  return (
    <aside aria-label="查询详情" className={styles.aside}>
      <section className={styles.section}>
        <div className={styles.label}>查询</div>
        <dl className={styles.kv}>
          <dt>输入</dt>
          <dd>{handle.query}</dd>
          {handle.normalized !== handle.query.trim() && (
            <>
              <dt>查询键</dt>
              <dd>{handle.normalized}</dd>
            </>
          )}
        </dl>
      </section>

      <section className={styles.section}>
        <div className={styles.label}>
          词典 · {hits}/{cards.length} 命中
        </div>
        <ul className={styles.list}>
          {cards.map((c) => (
            <li key={c.id} className={styles.row} data-status={c.status}>
              <span className={styles.name}>{c.name}</span>
              <span className={styles.status}>
                {c.status === "hit" && c.entries.length > 1 ? `${c.entries.length} 条` : STATUS_LABEL[c.status]}
                {c.ms != null && ` · ${c.ms < 10 ? c.ms.toFixed(1) : Math.round(c.ms)} ms`}
              </span>
            </li>
          ))}
        </ul>
      </section>
    </aside>
  );
}
