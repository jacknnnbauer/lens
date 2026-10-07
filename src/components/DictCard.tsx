// 词典卡片（SPEC 9.3、画板 05）：标题行是一个按钮，Space/Enter 展开折叠；折叠时显示一行摘要。

import { DictBody } from "../dict-render/DictBody";
import type { Card } from "../stores/lookup";
import styles from "./DictCard.module.css";
import { ChevronIcon } from "./icons";

interface Props {
  card: Card;
  expanded: boolean;
  onToggle: () => void;
  onEntry: (word: string) => void;
  onToggleKeyDown: (e: React.KeyboardEvent<HTMLButtonElement>) => void;
}

export function DictCard({ card, expanded, onToggle, onEntry, onToggleKeyDown }: Props) {
  const bodyId = `dict-body-${card.id}`;
  const summary = card.entries[0]?.summary ?? "";
  const pending = card.status === "pending";

  return (
    <article id={`dict-${card.id}`} className={styles.card} aria-busy={pending}>
      <h2 className={styles.heading}>
        <button
          type="button"
          className={styles.toggle}
          aria-expanded={expanded}
          aria-controls={bodyId}
          data-card-toggle={card.id}
          onClick={onToggle}
          onKeyDown={onToggleKeyDown}
        >
          <strong className={styles.name}>{card.name}</strong>
          <span className={styles.tag}>离线</span>
          {!expanded && summary && <span className={styles.summary}>{summary}</span>}
          {card.entries.length > 1 && <span className={styles.count}>{card.entries.length} 条</span>}
          <span className={styles.chevron}>
            <ChevronIcon open={expanded} />
          </span>
        </button>
      </h2>

      <div id={bodyId} hidden={!expanded}>
        {pending && (
          <div className={styles.skeleton} aria-label="正在查询">
            <span />
            <span />
            <span />
          </div>
        )}
        {card.status === "error" && (
          <p className={styles.error} role="alert">
            查询出错：{card.error}
          </p>
        )}
        {expanded &&
          card.entries.map((e, i) => (
            <div key={i} className={styles.entry}>
              <DictBody html={e.html} onEntry={onEntry} />
            </div>
          ))}
      </div>
    </article>
  );
}
