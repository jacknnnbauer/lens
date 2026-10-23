// 词汇网络：一个词在中间，派生词、同根词、易混词、近义词四组向外炸开；点任何一个词就查它。
// 数据全在本地算（Rust word_family）：词典词头前缀检索 + COCA 词表 / 近义词。

import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { tr } from "../i18n";
import { wordFamily, type WordFamily as Family } from "../ipc/lookup";
import styles from "./WordFamily.module.css";

const KINDS = [
  { kind: "derived", name: "派生词", hint: "同一个词干加后缀" },
  { kind: "root", name: "同根词", hint: "同一个词根换前缀" },
  { kind: "confusable", name: "易混词", hint: "拼写相近、容易用错" },
  { kind: "synonym", name: "近义词", hint: "COCA 近义词" },
] as const;
type Kind = (typeof KINDS)[number]["kind"];

const HEIGHT = 380;
const MAX_SHOWN = 8;

interface Node {
  word: string;
  kind: Kind;
  x: number;
  y: number;
  i: number;
}

/** 文字大概宽度（拉丁字母按 0.56em） */
const textWidth = (w: string, size: number) => w.length * size * 0.56 + 18;

export function WordFamily({ word, onLookup }: { word: string; onLookup: (w: string) => void }) {
  const [data, setData] = useState<Family | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [hidden, setHidden] = useState<Set<Kind>>(new Set());
  const [open, setOpen] = useState(true);
  const [burst, setBurst] = useState(false);
  const [hover, setHover] = useState<string | null>(null);
  const box = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(720);

  useEffect(() => {
    let alive = true;
    setData(null);
    setError(null);
    setBurst(false);
    wordFamily(word).then(
      (f) => {
        if (!alive) return;
        setData(f);
        // 先画在中心，下一帧再散开：有「炸开」的过渡
        requestAnimationFrame(() => requestAnimationFrame(() => alive && setBurst(true)));
      },
      (e) => alive && setError(String(e)),
    );
    return () => {
      alive = false;
    };
  }, [word]);

  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    setWidth(el.clientWidth || 720);
    const ro = new ResizeObserver(() => setWidth(el.clientWidth || 720));
    ro.observe(el);
    return () => ro.disconnect();
  }, [open]);

  const centerWord = data?.word ?? word;
  const groups = useMemo(() => (data?.groups ?? []).filter((g) => !hidden.has(g.kind as Kind)), [data, hidden]);

  // 每组占一段扇区（词多的组分到的角度大）；先按 2～3 圈交错摆，再把重叠的标签互相推开
  const nodes: Node[] = useMemo(() => {
    const cx = width / 2;
    const cy = HEIGHT / 2;
    const out: Node[] = [];
    const shown = groups.map((g) => g.words.slice(0, MAX_SHOWN));
    const sum = shown.reduce((s, w) => s + w.length + 1, 0);
    if (sum === 0) return out;
    const rx = Math.min(width / 2 - 70, 340);
    const ry = HEIGHT / 2 - 30;
    let a0 = -Math.PI / 2 - (Math.PI * 2 * ((shown[0]?.length ?? 0) + 1)) / sum / 2;
    groups.forEach((g, gi) => {
      const words = shown[gi];
      const sector = (Math.PI * 2 * (words.length + 1)) / sum;
      const m = words.length;
      words.forEach((w, i) => {
        const a = a0 + (sector * (i + 1)) / (m + 1);
        const ring = [0.58, 0.8, 0.98][i % (m > 4 ? 3 : 2)];
        out.push({ word: w, kind: g.kind as Kind, x: cx + Math.cos(a) * rx * ring, y: cy + Math.sin(a) * ry * ring, i: out.length });
      });
      a0 += sector;
    });
    // 推开重叠的标签（含中心词），并留在画布里
    const box = (n: { word: string; x: number; y: number }) => ({ w: textWidth(n.word, 13) + 8, h: 32 });
    // 中心词也占地方，别让相关词压在它上面
    const cw = textWidth(centerWord, 18) + 24;
    for (let iter = 0; iter < 80; iter++) {
      let moved = false;
      for (let i = 0; i < out.length; i++) {
        const a = out[i];
        const ba = box(a);
        // 与中心词
        const ox = (ba.w + cw) / 2 - Math.abs(a.x - cx);
        const oy = (ba.h + 44) / 2 - Math.abs(a.y - cy);
        if (ox > 0 && oy > 0) {
          if (ox < oy) a.x += (a.x >= cx ? 1 : -1) * ox;
          else a.y += (a.y >= cy ? 1 : -1) * oy;
          moved = true;
        }
        for (let j = i + 1; j < out.length; j++) {
          const b = out[j];
          const bb = box(b);
          const dx = (ba.w + bb.w) / 2 - Math.abs(a.x - b.x);
          const dy = (ba.h + bb.h) / 2 - Math.abs(a.y - b.y);
          if (dx > 0 && dy > 0) {
            // 沿重叠少的方向各退一半
            if (dx < dy) {
              const s = (a.x >= b.x ? 1 : -1) * (dx / 2 + 0.5);
              a.x += s;
              b.x -= s;
            } else {
              const s = (a.y >= b.y ? 1 : -1) * (dy / 2 + 0.5);
              a.y += s;
              b.y -= s;
            }
            moved = true;
          }
        }
        a.x = Math.min(Math.max(a.x, ba.w / 2 + 6), width - ba.w / 2 - 6);
        a.y = Math.min(Math.max(a.y, 20), HEIGHT - 20);
      }
      if (!moved) break;
    }
    return out;
  }, [groups, width, centerWord]);

  const total = data?.groups.reduce((s, g) => s + g.words.length, 0) ?? 0;
  const cx = width / 2;
  const cy = HEIGHT / 2;

  return (
    <section className={styles.card} aria-label={tr("词汇网络")} data-ui="card">
      <header className={styles.head}>
        <button type="button" className={styles.title} aria-expanded={open} onClick={() => setOpen((o) => !o)}>
          {tr("词汇网络")}
          {data && <small>{tr("{n} 个相关词", { n: total })}</small>}
        </button>
        {data?.root && open && <span className={styles.root}>{tr("词根 {root}", { root: data.root })}</span>}
      </header>

      {open && (
        <>
          <div ref={box} className={styles.canvas} data-ui="canvas">
            {error && <p className={styles.note}>{error}</p>}
            {!data && !error && <p className={styles.note}>{tr("整理中…")}</p>}
            {data && total === 0 && <p className={styles.note}>{tr("本地词典里没找到相关的词")}</p>}
            {data && total > 0 && (
              <svg width={width} height={HEIGHT} viewBox={`0 0 ${width} ${HEIGHT}`} role="group" aria-label={tr("「{word}」的相关词", { word: data.word })}>
                {nodes.map((n) => (
                  <line
                    key={`l-${n.word}-${n.kind}`}
                    className={styles.edge}
                    data-kind={n.kind}
                    data-on={hover === n.word || undefined}
                    x1={cx}
                    y1={cy}
                    x2={n.x}
                    y2={n.y}
                    data-burst={burst || undefined}
                    style={{ transitionDelay: `${n.i * 22 + 120}ms` }}
                  />
                ))}
                {nodes.map((n) => {
                  const w = textWidth(n.word, 13);
                  return (
                    <g
                      key={`n-${n.word}-${n.kind}`}
                      className={styles.node}
                      data-kind={n.kind}
                      style={{ transform: burst ? `translate(${n.x}px, ${n.y}px)` : `translate(${cx}px, ${cy}px) scale(0.3)`, opacity: burst ? 1 : 0, transitionDelay: `${n.i * 22}ms` }}
                      onMouseEnter={() => setHover(n.word)}
                      onMouseLeave={() => setHover(null)}
                    >
                      <a
                        href={`#${n.word}`}
                        aria-label={tr("查「{word}」", { word: n.word })}
                        onClick={(e) => {
                          e.preventDefault();
                          onLookup(n.word);
                        }}
                      >
                        <rect x={-w / 2} y={-13} width={w} height={26} rx={13} />
                        <text textAnchor="middle" dominantBaseline="central">
                          {n.word}
                        </text>
                      </a>
                    </g>
                  );
                })}
                <g className={styles.center} transform={`translate(${cx}, ${cy})`}>
                  <rect x={-textWidth(data.word, 18) / 2 - 6} y={-19} width={textWidth(data.word, 18) + 12} height={38} rx={19} />
                  <text textAnchor="middle" dominantBaseline="central">
                    {data.word}
                  </text>
                </g>
              </svg>
            )}
          </div>
          {data && total > 0 && (
            <div className={styles.legend} role="group" aria-label={tr("显示哪些相关词")}>
              {KINDS.map(({ kind, name, hint }) => {
                const count = data.groups.find((g) => g.kind === kind)?.words.length ?? 0;
                if (count === 0) return null;
                return (
                  <button
                    key={kind}
                    type="button"
                    data-kind={kind}
                    aria-pressed={!hidden.has(kind)}
                    title={tr(hint)}
                    onClick={() =>
                      setHidden((h) => {
                        const next = new Set(h);
                        if (next.has(kind)) next.delete(kind);
                        else next.add(kind);
                        return next;
                      })
                    }
                  >
                    <i />
                    {tr(name)} {count}
                  </button>
                );
              })}
            </div>
          )}
        </>
      )}
    </section>
  );
}
