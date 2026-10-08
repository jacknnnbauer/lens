// 义项图谱（M8 实验、画板 01）：把各词典的义项对齐成组，显示共识与差异。
// 抽取与对齐在 Rust（senses.rs）；这里只负责三种视图：综合 / 逐典 / 对比。

import { useEffect, useMemo, useState } from "react";
import { tr } from "../i18n";
import { senseAtlas, type SenseAtlas as Atlas } from "../ipc/lookup";
import type { Card } from "../stores/lookup";
import styles from "./SenseAtlas.module.css";

type View = "merged" | "byDict" | "compare";
const VIEWS: Array<{ id: View; name: string }> = [
  { id: "merged", name: "综合" },
  { id: "byDict", name: "逐典" },
  { id: "compare", name: "对比" },
];
const COMPARE_ROWS = 12;

/** 词典徽标：中文名取第一个汉字，英文名取前两个词的首字母 */
export function dictBadge(name: string): string {
  const cjk = /[一-鿿]/.exec(name);
  if (cjk) return cjk[0];
  const words = name.split(/[\s\-–—'’]+/).filter((w) => /^[A-Za-z]/.test(w));
  return words
    .slice(0, 2)
    .map((w) => w[0].toUpperCase())
    .join("") || name.slice(0, 1);
}

interface Props {
  word: string;
  cards: Card[];
  onJump: (dictId: string) => void;
}

export function SenseAtlas({ word, cards, onJump }: Props) {
  const [atlas, setAtlas] = useState<Atlas | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [view, setView] = useState<View>("merged");
  const [open, setOpen] = useState<number | null>(null);
  const [showUnique, setShowUnique] = useState(false);
  const [collapsed, setCollapsed] = useState(false);

  const hits = useMemo(() => cards.filter((c) => c.status === "hit"), [cards]);
  const key = hits.map((c) => `${c.id}:${c.entries.length}`).join("|");

  useEffect(() => {
    let alive = true;
    setOpen(null);
    setShowUnique(false);
    if (hits.length < 2) {
      setAtlas(null);
      return;
    }
    senseAtlas(
      word,
      hits.map((c) => ({ id: c.id, name: c.name, entries: c.entries.map((e) => e.html) })),
    ).then(
      (a) => alive && (setAtlas(a), setError(null)),
      (e) => alive && setError(String(e)),
    );
    return () => {
      alive = false;
    };
    // 只在命中的词典或词条数量变化时重算（hits 每次渲染都是新数组）
  }, [word, key]);

  if (error) return null;
  if (!atlas || atlas.dicts.length < 2) return null;

  const badges = atlas.dicts.map((d) => dictBadge(d.name));
  const n = atlas.dicts.length;
  const shared = atlas.clusters.filter((c) => c.dicts.length > 1);
  const unique = atlas.clusters.filter((c) => c.dicts.length === 1);
  const sharedNo = new Map(shared.map((c, i) => [c, i + 1]));

  const badge = (d: number, key?: number) => (
    <button key={key} type="button" className={styles.badge} title={tr("跳到「{name}」", { name: atlas.dicts[d].name })} onClick={() => onJump(atlas.dicts[d].id)}>
      {badges[d]}
    </button>
  );

  return (
    <section className={styles.atlas} aria-labelledby="atlas-title">
      <header className={styles.head}>
        <h2 id="atlas-title">
          {tr("义项图谱")} <small className={styles.exp}>{tr("实验")}</small>
        </h2>
        <span className={styles.summary}>{tr("{n} 部词典 · 聚合为 {m} 个义项", { n, m: atlas.clusters.length })}</span>
        {!collapsed && (
          <div role="tablist" aria-label={tr("义项图谱视图")} className={styles.tabs}>
            {VIEWS.map((v) => (
              <button key={v.id} type="button" role="tab" aria-selected={view === v.id} onClick={() => setView(v.id)}>
                {tr(v.name)}
              </button>
            ))}
          </div>
        )}
        <button type="button" className={styles.link} aria-expanded={!collapsed} onClick={() => setCollapsed((c) => !c)}>
          {tr(collapsed ? "展开" : "收起")}
        </button>
      </header>

      {!collapsed && view === "merged" && (
        <>
          {shared.length === 0 && <p className={styles.hint}>{tr("几部词典的义项没有对上（释义写法差别较大）。可以看「逐典」。")}</p>}
          <ol className={styles.list}>
            {shared.map((c, i) => (
              <li key={i} className={styles.item}>
                <div className={styles.row}>
                  <span className={styles.no}>{i + 1}</span>
                  <button type="button" className={styles.label} aria-expanded={open === i} onClick={() => setOpen((o) => (o === i ? null : i))}>
                    {c.label}
                  </button>
                  <span className={styles.badges}>
                    {c.dicts.map((d) => badge(d, d))}
                  </span>
                  <span className={styles.consensus} data-full={c.dicts.length === n || undefined}>
                    {tr("共识 {k}/{n}", { k: c.dicts.length, n })}
                  </span>
                </div>
                {c.posDiff.length > 0 && (
                  <p className={styles.diff}>
                    {tr("差异：")}
                    {c.posDiff.map(([d, p]) => `${atlas.dicts[d].name} ${p}`).join(tr("；"))}
                  </p>
                )}
                {open === i && (
                  <ul className={styles.members}>
                    {c.senses.map((s, j) => (
                      <li key={j}>
                        {badge(s.dict)}
                        <span>
                          {s.pos && <em>{s.pos} </em>}
                          {s.text}
                        </span>
                      </li>
                    ))}
                  </ul>
                )}
              </li>
            ))}
          </ol>
          {unique.length > 0 && (
            <div className={styles.uniqueBox}>
              <button type="button" className={styles.link} aria-expanded={showUnique} onClick={() => setShowUnique((s) => !s)}>
                {tr(showUnique ? "收起独有义项" : "独有义项 {n} 条", { n: unique.length })}
              </button>
              {showUnique && (
                <ul className={styles.members}>
                  {unique.map((c, i) => (
                    <li key={i}>
                      {badge(c.dicts[0])}
                      <span>
                        {c.senses[0].pos && <em>{c.senses[0].pos} </em>}
                        {c.label}
                      </span>
                      <small className={styles.tag}>{c.domain ? tr("独有 · {d}", { d: c.domain }) : tr("独有")}</small>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          )}
        </>
      )}

      {!collapsed && view === "byDict" && (
        <div className={styles.byDict}>
          {atlas.dicts.map((d, di) => (
            <div key={d.id} className={styles.dictCol}>
              <h3>
                {badge(di)} {d.name}
                <small>{tr("{n} 条", { n: d.senses })}</small>
              </h3>
              <ol>
                {atlas.clusters
                  .flatMap((c) => c.senses.filter((s) => s.dict === di).map((s) => ({ s, c })))
                  .sort((x, y) => x.s.index - y.s.index)
                  .map(({ s, c }) => (
                    <li key={s.index}>
                      {sharedNo.has(c) ? <span className={styles.ref}>#{sharedNo.get(c)}</span> : <span className={styles.ref} data-unique>{tr("独有")}</span>}
                      {s.text}
                    </li>
                  ))}
              </ol>
            </div>
          ))}
        </div>
      )}

      {!collapsed && view === "compare" && (
        <div className={styles.tableWrap}>
          <table className={styles.table}>
            <thead>
              <tr>
                <th scope="col">{tr("义项")}</th>
                {atlas.dicts.map((d, di) => (
                  <th key={d.id} scope="col" title={d.name}>
                    {badge(di)}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {shared.slice(0, COMPARE_ROWS).map((c, i) => (
                <tr key={i}>
                  <th scope="row">
                    <span className={styles.no}>{i + 1}</span> {c.label}
                  </th>
                  {atlas.dicts.map((d, di) => {
                    const s = c.senses.find((x) => x.dict === di);
                    return (
                      <td key={d.id} title={s?.text ?? ""} data-on={s ? true : undefined}>
                        {s ? "●" : "·"}
                      </td>
                    );
                  })}
                </tr>
              ))}
            </tbody>
          </table>
          {shared.length > COMPARE_ROWS && <p className={styles.hint}>{tr("只列共识最多的 {n} 组", { n: COMPARE_ROWS })}</p>}
        </div>
      )}
    </section>
  );
}
