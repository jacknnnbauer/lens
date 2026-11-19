// 词汇量测试的结果卡片：数字和误差范围、在参照刻度上的位置、阶段评价、各词频段的认识情况、答题表现。

import { tr } from "../../i18n";
import type { VocabEstimate } from "../../ipc/vocabTest";
import { useSettings } from "../../stores/settings";
import { chooseLevel } from "../../routes/reader/levelPrefs";
import { levelInfo, MILESTONES, scalePos } from "./levelInfo";
import styles from "./VocabResult.module.css";

const fmt = (n: number) => n.toLocaleString("en-US");
const hundreds = (n: number) => fmt(Math.round(n / 100) * 100);
const pct = (x: number) => `${Math.round(Math.max(0, Math.min(1, x)) * 100)}%`;

function segmentName(from: number, to: number, last: boolean): string {
  if (from <= 1) return tr("最常用 {n} 词", { n: fmt(to) });
  if (last) return tr("{n} 以后", { n: fmt(from - 1) });
  return `${fmt(from - 1)}–${fmt(to)}`;
}

/** 参照刻度：初中、高中……母语者；误差范围是一段浅色带，自己的位置是一个点 */
function Scale({ value, low, high }: { value: number; low: number; high: number }) {
  const at = (v: number) => `${(scalePos(v) * 100).toFixed(2)}%`;
  return (
    <div className={styles.scale} aria-hidden="true">
      <div className={styles.track}>
        <span className={styles.range} style={{ left: at(low), right: `calc(100% - ${at(high)})` }} />
        {MILESTONES.map((m) => (
          <i key={m.at} className={styles.tick} style={{ left: at(m.at) }} data-passed={value >= m.at || undefined} />
        ))}
        <span className={styles.dot} style={{ left: at(value) }} />
      </div>
      <div className={styles.labels}>
        {MILESTONES.map((m, i) => (
          <span key={m.at} style={{ left: at(m.at) }} data-row={i % 2} data-passed={value >= m.at || undefined} title={`${tr(m.label)} ≈ ${fmt(m.at)}`}>
            {tr(m.label)}
          </span>
        ))}
      </div>
    </div>
  );
}

export function VocabResult({ result, onClose, closeLabel }: { result: VocabEstimate; onClose: () => void; closeLabel: string }) {
  const vocab = useSettings((s) => s.settings?.vocab);
  const update = useSettings((s) => s.update);
  const info = levelInfo(result.value);
  const s = result.stats;
  return (
    <div className={styles.card}>
      <header className={styles.head}>
        <p className={styles.eyebrow}>{tr("你的词汇量")}</p>
        <p className={styles.value}>{result.capped ? `${fmt(result.value)}+` : hundreds(result.value)}</p>
        {!result.capped && result.value > 0 && <p className={styles.interval}>{tr("误差范围 {low} – {high}", { low: hundreds(result.low), high: hundreds(result.high) })}</p>}
        <p className={styles.badge}>{tr(info.title)}</p>
        <p className={styles.summary}>{tr(info.summary)}</p>
        {result.optimistic && <p className={styles.warn}>{tr("这次答题可能偏乐观，建议重测。")}</p>}
      </header>

      <Scale value={result.value} low={result.low} high={result.high} />

      <div className={styles.grid}>
        <section className={styles.panel}>
          <h3>{tr("词汇分布")}</h3>
          <ul className={styles.bars}>
            {result.profile.map((g, i) => (
              <li key={g.from}>
                <span className={styles.barName}>{segmentName(g.from, g.to, i === result.profile.length - 1)}</span>
                <span className={styles.barTrack}>
                  <span style={{ width: pct(g.known) }} />
                </span>
                <span className={styles.barPct}>{pct(g.known)}</span>
              </li>
            ))}
          </ul>
        </section>
        <section className={styles.panel}>
          <h3>{tr("答题表现")}</h3>
          <dl className={styles.stats}>
            <dt>{tr("认识的真词")}</dt>
            <dd>
              {s.realKnown}
              <small> / {s.real}</small>
            </dd>
            <dt>{tr("识破的假词")}</dt>
            <dd>
              {s.fakesCaught}
              <small> / {s.fakes}</small>
            </dd>
            {s.checks > 0 && (
              <>
                <dt>{tr("释义抽查答对")}</dt>
                <dd>
                  {s.checksRight}
                  <small> / {s.checks}</small>
                </dd>
              </>
            )}
          </dl>
        </section>
      </div>

      <dl className={styles.advice}>
        <dt>{tr("现在适合读")}</dt>
        <dd>{tr(info.canRead)}</dd>
        <dt>{tr("下一步")}</dt>
        <dd>{tr(info.next)}</dd>
      </dl>

      <footer className={styles.foot}>
        {vocab?.follow ? (
          <p className={styles.note}>{tr("已设为阅读的生词门槛，排名在这之后的词会被标出来。")}</p>
        ) : (
          <button type="button" className={styles.ghost} onClick={() => update({ vocab: chooseLevel("me", vocab?.estimate ?? result.value) })}>
            {tr("设为阅读门槛")}
          </button>
        )}
        <button type="button" className={styles.primary} onClick={onClose}>
          {closeLabel}
        </button>
      </footer>
      <p className={styles.fine}>{tr("参照点是各类考纲或课标要求的词数，计算口径和本测试不完全相同，仅供大致参考。")}</p>
    </div>
  );
}
