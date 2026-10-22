// 词典卡片（SPEC 9.3、画板 05）：始终展开，不折叠（再长也完整显示）。
// 标题可获得焦点，Tab / Shift+Tab、J / K 在卡片之间跳转。

import type { CopyFormat } from "../copy/convert";
import { isVocabularyCom, VocabExamples } from "./VocabExamples";
import { DictBody } from "../dict-render/DictBody";
import { openWebDict } from "../ipc/online";
import type { Card } from "../stores/lookup";
import styles from "./DictCard.module.css";
import { CopyMenu } from "./CopyMenu";
import { tr } from "../i18n";

interface Props {
  card: Card;
  onEntry: (word: string) => void;
  onToggleKeyDown: (e: React.KeyboardEvent<HTMLElement>) => void;
  copyFormat: CopyFormat;
  onCopy: (format: CopyFormat) => void;
  onSetDefaultFormat: (format: CopyFormat) => void;
  /** 在线词典「手动」查询 */
  onQueryOnline: () => void;
}

/** 只标在线来源；本地词典是默认情况，不加标签 */
const SOURCE_TAG = { local: null, api: "在线", web: "网页" } as const;

function fmtDate(ms: number): string {
  const d = new Date(ms);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

export function DictCard({ card, onEntry, onToggleKeyDown, copyFormat, onCopy, onSetDefaultFormat, onQueryOnline }: Props) {
  const pending = card.status === "pending";

  return (
    <article id={`dict-${card.id}`} className={styles.card} aria-busy={pending}>
      <div className={styles.header} data-wall-handle="">
        {/* 标题可聚焦，作为卡片间跳转的落点（不是按钮：卡片不折叠） */}
        <h2 className={styles.heading} tabIndex={0} data-card-toggle={card.id} onKeyDown={onToggleKeyDown}>
          <span className={styles.toggle}>
            <strong className={styles.name} title={card.name}>
              {card.name}
            </strong>
            {SOURCE_TAG[card.source] && <span className={styles.tag}>{tr(SOURCE_TAG[card.source] ?? "")}</span>}
            {card.cachedAt != null && (
              <span className={styles.tag} data-stale={card.stale || undefined} title={card.stale ? tr("联网失败：{error}", { error: card.error ?? "" }) : undefined}>
                {tr(card.stale ? "离线缓存" : "缓存")} · {fmtDate(card.cachedAt)}
              </span>
            )}
            {card.entries.length > 1 && <span className={styles.count}>{tr("{n} 条", { n: card.entries.length })}</span>}
          </span>
        </h2>
        {card.source !== "web" && (
          <CopyMenu
            label={tr("「{name}」", { name: card.name })}
            defaultFormat={copyFormat}
            disabled={card.status !== "hit"}
            onCopy={onCopy}
            onSetDefault={onSetDefaultFormat}
          />
        )}
      </div>

      <div className={styles.body}>
        {pending && (
          <div className={styles.skeleton} aria-label={tr("正在查询")}>
            <span />
            <span />
            <span />
          </div>
        )}
        {card.status === "manual" && (
          <p className={styles.online}>
            {tr("这部在线词典设为手动查询。")}
            <button type="button" className={styles.onlineBtn} onClick={onQueryOnline}>
              {tr("查询")}
            </button>
          </p>
        )}
        {card.status === "web" && card.webUrl && (
          <p className={styles.online}>
            {tr("网页词典在单独的只读窗口里打开原页面。")}
            <button type="button" className={styles.onlineBtn} onClick={() => void openWebDict(card.webUrl ?? "", card.name)}>
              {tr("打开网页 ↗")}
            </button>
          </p>
        )}
        {card.status === "error" && (
          <p className={styles.error} role="alert">
            {tr("查询出错：{error}", { error: card.error ?? "" })}
          </p>
        )}
        {card.entries.map((e, i) => (
          <div key={i} className={styles.entry}>
            <DictBody html={e.html} onEntry={onEntry} dictId={card.id} />
          </div>
        ))}
        {card.status === "hit" && card.entries[0] && isVocabularyCom(card.entries[0].html) && (
          <VocabExamples word={card.entries[0].headword || card.entries[0].html.match(/data-word="([^"]+)"/)?.[1] || ""} />
        )}
      </div>
    </article>
  );
}
