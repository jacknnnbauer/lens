// 卡片墙（瀑布流）：每张卡片保持自身高度，按顺序放进当前最矮的一列。
// 第一排（前 N 张）顶端对齐；同一列里卡片上下紧挨，各列底部可以参差。
// 不用 CSS columns：那是先排满第一列再排第二列，阅读顺序会变成竖着走。
// 卡片用绝对定位摆放，换位置时不重新挂载（词条 Shadow DOM 不用重绘）。

import { useEffect, useLayoutEffect, useMemo, useReducer, useRef, useState, type ReactNode } from "react";
import styles from "./CardWall.module.css";

interface Item {
  key: string;
  node: ReactNode;
}

interface Props {
  items: Item[];
  /** 每列最小宽度（px） */
  minColumn: number;
  gap?: number;
}

export function CardWall({ items, minColumn, gap = 14 }: Props) {
  const box = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);
  const heights = useRef(new Map<string, number>());
  const nodes = useRef(new Map<string, HTMLElement>());
  const [, remeasured] = useReducer((n: number) => n + 1, 0);

  // 卡片高度变化（词条加载完、展开折叠、窗口变窄换行）→ 重新排
  const observer = useMemo(
    () =>
      typeof ResizeObserver === "undefined"
        ? null
        : new ResizeObserver((entries) => {
            let changed = false;
            for (const e of entries) {
              const key = (e.target as HTMLElement).dataset.wallKey;
              if (!key) continue;
              const h = Math.round(e.borderBoxSize?.[0]?.blockSize ?? (e.target as HTMLElement).offsetHeight);
              if (heights.current.get(key) !== h) {
                heights.current.set(key, h);
                changed = true;
              }
            }
            if (changed) remeasured();
          }),
    [],
  );
  useEffect(() => () => observer?.disconnect(), [observer]);

  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    setWidth(el.clientWidth);
    const ro = new ResizeObserver(() => setWidth(el.clientWidth));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // 已经不在列表里的卡片：不再观察、忘掉高度（每次渲染内联 ref 会先收到 null，所以不在那里清）
  useEffect(() => {
    const live = new Set(items.map((i) => i.key));
    for (const [k, el] of nodes.current) {
      if (!live.has(k)) {
        observer?.unobserve(el);
        nodes.current.delete(k);
        heights.current.delete(k);
      }
    }
  });

  const cols = Math.max(1, Math.floor((width + gap) / (minColumn + gap)));
  const colWidth = cols > 0 && width > 0 ? (width - gap * (cols - 1)) / cols : 0;

  // 依次放进最矮的一列（一样高时取最左边），所以第一排从左到右是第 1…N 张
  const colHeights = new Array<number>(cols).fill(0);
  const placed = items.map((it) => {
    let c = 0;
    for (let i = 1; i < cols; i++) if (colHeights[i] < colHeights[c] - 0.5) c = i;
    const top = colHeights[c];
    colHeights[c] += (heights.current.get(it.key) ?? 0) + gap;
    return { ...it, left: c * (colWidth + gap), top };
  });
  const total = Math.max(0, ...colHeights.map((h) => h - gap));

  return (
    <div ref={box} className={styles.wall} style={{ height: total }}>
      {width > 0 &&
        placed.map((p) => (
          <div
            key={p.key}
            data-wall-key={p.key}
            className={styles.item}
            style={{ width: colWidth, transform: `translate(${p.left}px, ${p.top}px)` }}
            ref={(el) => {
              if (!observer) return;
              const prev = nodes.current.get(p.key);
              if (prev && prev !== el) observer.unobserve(prev);
              if (el) {
                nodes.current.set(p.key, el);
                observer.observe(el);
              }
            }}
          >
            {p.node}
          </div>
        ))}
    </div>
  );
}
