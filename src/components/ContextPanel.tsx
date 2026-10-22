// 右侧栏：按设置 › 布局「右侧栏内容」的顺序显示——COCA 词频与搭配、真实例句、AI 双语例句、
// 查询、各词典命中情况，以及用户加进来的词典释义（dict:<id>）。

import { DictBody } from "../dict-render/DictBody";
import { tr } from "../i18n";
import { useLookup, type Card } from "../stores/lookup";
import { briefOf } from "../stores/save";
import { useSettings } from "../stores/settings";
import styles from "./ContextPanel.module.css";
import { dictIdOf, normalizePanel } from "./panelSections";
import { AiSection, CocaSection, VocabSection } from "./WordInsights";

const STATUS_LABEL = {
  pending: "查询中…",
  hit: "命中",
  miss: "未收录",
  error: "出错",
  manual: "待手动查询",
  web: "网页",
  skipped: "本地已命中，未查",
} as const;

/** 右侧栏里的一本词典：当前查询在这本词典里的词条 */
function DictSection({ dictId, cards, onEntry }: { dictId: string; cards: Card[]; onEntry: (w: string) => void }) {
  const card = cards.find((c) => c.id === dictId);
  if (!card) return null;
  return (
    <section className={styles.section} aria-label={card.name} data-word-lookup="">
      <div className={styles.label}>{card.name}</div>
      {card.status === "hit" ? (
        <div className={styles.dict}>
          {card.entries.map((e, i) => (
            <DictBody key={`${dictId}-${i}`} html={e.html} onEntry={onEntry} dictId={dictId} />
          ))}
        </div>
      ) : (
        <p className={styles.note}>{tr(STATUS_LABEL[card.status])}</p>
      )}
    </section>
  );
}

/** onLookup：点词典里的词条链接时查词（主窗口的查词，会同步搜索框与历史） */
export function ContextPanel({ onLookup }: { onLookup: (w: string) => void }) {
  const handle = useLookup((s) => s.handle);
  const cards = useLookup((s) => s.cards);
  const done = useLookup((s) => s.done);
  const panel = normalizePanel(useSettings((s) => s.settings?.layout.panel));

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
  // COCA / 例句 / AI 等查询有结果后再显示，避免对着半截输入发请求
  const ready = !!word && (done || hits > 0);
  const key = word.toLowerCase();

  const render = (id: string) => {
    const dict = dictIdOf(id);
    if (dict) return <DictSection key={id} dictId={dict} cards={cards} onEntry={onLookup} />;
    switch (id) {
      case "coca":
        return ready ? <CocaSection key={`coca-${key}`} word={word} /> : null;
      case "vocab":
        return ready ? <VocabSection key={`vocab-${key}`} word={word} /> : null;
      case "ai":
        return ready ? <AiSection key={`ai-${key}`} word={word} brief={briefOf(cards, word)} /> : null;
      case "query":
        return (
          <section key="query" className={styles.section}>
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
        );
      case "dicts":
        return (
          <section key="dicts" className={styles.section}>
            <div className={styles.label}>{tr("词典 · {hits}/{total} 命中", { hits, total: cards.length })}</div>
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
        );
      default:
        return null;
    }
  };

  return (
    <aside aria-label={tr("查询详情")} className={styles.aside}>
      {panel.filter((p) => p.show).map((p) => render(p.id))}
    </aside>
  );
}
