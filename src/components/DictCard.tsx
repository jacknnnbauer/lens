// 词典卡片（SPEC 9.3、画板 05）：标题行是一个按钮，Space/Enter 展开折叠；折叠时显示一行摘要。

import type { CopyFormat } from "../copy/convert";
import { DictBody } from "../dict-render/DictBody";
import { openWebDict } from "../ipc/online";
import type { Card } from "../stores/lookup";
import styles from "./DictCard.module.css";
import { CopyMenu } from "./CopyMenu";
import { ChevronIcon } from "./icons";

interface Props {
  card: Card;
  expanded: boolean;
  onToggle: () => void;
  onEntry: (word: string) => void;
  onToggleKeyDown: (e: React.KeyboardEvent<HTMLButtonElement>) => void;
  copyFormat: CopyFormat;
  onCopy: (format: CopyFormat) => void;
  onSetDefaultFormat: (format: CopyFormat) => void;
  /** 在线词典「手动」查询 */
  onQueryOnline: () => void;
}

const SOURCE_TAG = { local: "离线", api: "在线", web: "网页" } as const;

function fmtDate(ms: number): string {
  const d = new Date(ms);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

export function DictCard({ card, expanded, onToggle, onEntry, onToggleKeyDown, copyFormat, onCopy, onSetDefaultFormat, onQueryOnline }: Props) {
  const bodyId = `dict-body-${card.id}`;
  const summary = card.entries[0]?.summary ?? "";
  const pending = card.status === "pending";

  return (
    <article id={`dict-${card.id}`} className={styles.card} aria-busy={pending}>
      <div className={styles.header}>
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
            <span className={styles.tag}>{SOURCE_TAG[card.source]}</span>
            {card.cachedAt != null && (
              <span className={styles.tag} data-stale={card.stale || undefined} title={card.stale ? `联网失败：${card.error ?? ""}` : undefined}>
                {card.stale ? "离线缓存" : "缓存"} · {fmtDate(card.cachedAt)}
              </span>
            )}
            {!expanded && summary && <span className={styles.summary}>{summary}</span>}
            {card.entries.length > 1 && <span className={styles.count}>{card.entries.length} 条</span>}
            <span className={styles.chevron}>
              <ChevronIcon open={expanded} />
            </span>
          </button>
        </h2>
        {card.source !== "web" && (
          <CopyMenu
            label={`「${card.name}」`}
            defaultFormat={copyFormat}
            disabled={card.status !== "hit"}
            onCopy={onCopy}
            onSetDefault={onSetDefaultFormat}
          />
        )}
      </div>

      <div id={bodyId} hidden={!expanded}>
        {pending && (
          <div className={styles.skeleton} aria-label="正在查询">
            <span />
            <span />
            <span />
          </div>
        )}
        {card.status === "manual" && (
          <p className={styles.online}>
            这部在线词典设为手动查询。
            <button type="button" className={styles.onlineBtn} onClick={onQueryOnline}>
              查询
            </button>
          </p>
        )}
        {card.status === "web" && card.webUrl && (
          <p className={styles.online}>
            网页词典在单独的只读窗口里打开原页面。
            <button type="button" className={styles.onlineBtn} onClick={() => void openWebDict(card.webUrl ?? "", card.name)}>
              打开网页 ↗
            </button>
          </p>
        )}
        {card.status === "error" && (
          <p className={styles.error} role="alert">
            查询出错：{card.error}
          </p>
        )}
        {expanded &&
          card.entries.map((e, i) => (
            <div key={i} className={styles.entry}>
              <DictBody html={e.html} onEntry={onEntry} dictId={card.id} />
            </div>
          ))}
      </div>
    </article>
  );
}
