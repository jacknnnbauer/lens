// 生词本顶部的词汇量卡片：当前数字、本月增长、成长曲线，可以重新测试。

import { useCallback, useEffect, useState } from "react";
import { tr } from "../../i18n";
import { localDay, vocabSummary, type VocabSummary } from "../../ipc/vocabTest";
import { syncVocab } from "../../stores/vocabSync";
import { Hint } from "../Hint";
import { buildChart, daysBetween, type Chart, type ChartRange } from "./growthChart";
import { VocabTestOverlay } from "./VocabTest";
import styles from "./GrowthCard.module.css";

/** 距上次测试满这么多天，轻提示可以重测 */
const RETEST_DAYS = 90;
const RANGES: [ChartRange, string][] = [
  [30, "30 天"],
  [90, "90 天"],
  ["all", "全部"],
];
const W = 640;
const H = 120;
const PAD = 8;

function Curve({ chart }: { chart: Chart }) {
  const span = Math.max(1, chart.points.length - 1);
  const x = (v: number) => PAD + (v / span) * (W - PAD * 2);
  const y = (v: number) => H - PAD - ((v - chart.min) / (chart.max - chart.min)) * (H - PAD * 2);
  const line = chart.points.map((p) => `${x(p.x).toFixed(1)},${y(p.y).toFixed(1)}`).join(" ");
  return (
    <svg className={styles.chart} viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" role="img" aria-label={tr("词汇量成长曲线")}>
      <polyline className={styles.line} points={line} vectorEffect="non-scaling-stroke" />
      {chart.dots.map((d) => (
        <circle key={d.x} className={styles.dot} cx={x(d.x)} cy={y(d.y)} r={4}>
          <title>{tr("测试结果 {n}", { n: d.value.toLocaleString() })}</title>
        </circle>
      ))}
    </svg>
  );
}

export function GrowthCard({ active }: { active: boolean }) {
  const [data, setData] = useState<VocabSummary | null | undefined>(undefined);
  const [range, setRange] = useState<ChartRange>(30);
  const [testing, setTesting] = useState(false);

  const load = useCallback(async () => {
    await syncVocab();
    setData(await vocabSummary().catch(() => null));
  }, []);
  useEffect(() => {
    if (active) void load();
  }, [active, load]);

  const test = (
    <>
      {testing && (
        <VocabTestOverlay
          onClose={() => {
            setTesting(false);
            void load();
          }}
        />
      )}
    </>
  );
  if (data === undefined) return null;

  if (data === null) {
    return (
      <section className={styles.card} aria-label={tr("我的词汇量")}>
        <div className={styles.empty}>
          <div>
            <strong>{tr("我的词汇量")}</strong>
            <p>{tr("三分钟测出你的词汇量，阅读时自动标出生词，之后这里会记录你的成长。")}</p>
          </div>
          <button type="button" className={styles.primary} data-hint="vocab-test" onClick={() => setTesting(true)}>
            {tr("测一下我的词汇量")}
          </button>
        </div>
        <Hint id="vocab-test" anchor="vocab-test" show={active}>
          {tr("新功能：测出词汇量后，阅读时会按你的水平标生词。")}
        </Hint>
        {test}
      </section>
    );
  }

  const today = localDay();
  const since = daysBetween(data.lastTest.day, today);
  const chart = buildChart(data.days, data.tests, range, today);
  return (
    <section className={styles.card} aria-label={tr("我的词汇量")}>
      <div className={styles.head}>
        <div>
          <span className={styles.label}>{tr("我的词汇量")}</span>
          <strong className={styles.value}>{data.current.toLocaleString()}</strong>
          <span className={styles.meta}>
            {tr("本月 {n}", { n: `${data.monthGain >= 0 ? "+" : ""}${data.monthGain}` })} · {since === 0 ? tr("今天测试过") : tr("距上次测试 {n} 天", { n: since })}
          </span>
        </div>
        <button type="button" className={styles.ghost} onClick={() => setTesting(true)}>
          {tr("重新测试")}
        </button>
      </div>
      {since >= RETEST_DAYS && <p className={styles.tip}>{tr("距上次测试已经很久了，可以重测一次校准。")}</p>}
      <Curve chart={chart} />
      <div role="group" aria-label={tr("时间范围")} className={styles.ranges}>
        {RANGES.map(([r, name]) => (
          <button key={String(r)} type="button" aria-pressed={range === r} onClick={() => setRange(r)}>
            {tr(name)}
          </button>
        ))}
      </div>
      {test}
    </section>
  );
}
