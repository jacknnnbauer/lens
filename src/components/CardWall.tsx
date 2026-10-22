// 卡片墙（瀑布流）：每张卡片保持自身高度，按顺序放进当前最矮的一列。
// 第一排（前 N 张）顶端对齐；同一列里卡片上下紧挨，各列底部可以参差。
// 不用 CSS columns：那是先排满第一列再排第二列，阅读顺序会变成竖着走。
// 卡片用绝对定位摆放，换位置时不重新挂载（词条 Shadow DOM 不用重绘）。
// 给了 onReorder 时可以拖动换位：按住卡片里标了 data-wall-handle 的地方（词典卡片的标题栏）拖，
// 移动超过 5px 才算拖动（单击、选字不受影响），松手放到指针下那张卡片的前面或后面。

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
  /** 拖动换位：把 key 放到 target 之前（after = 之后） */
  onReorder?: (key: string, target: string, after: boolean) => void;
}

const DRAG_START_PX = 5;

interface Drag {
  key: string;
  /** 按下时的指针位置 */
  x0: number;
  y0: number;
  dx: number;
  dy: number;
  active: boolean;
  over: string | null;
  after: boolean;
}

export function CardWall({ items, minColumn, gap = 14, onReorder }: Props) {
  const [drag, setDrag] = useState<Drag | null>(null);
  const dragRef = useRef<Drag | null>(null);
  dragRef.current = drag;
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

  // 拖动：在 wall 上监听指针移动 / 松开（指针离开卡片也能继续）
  const placedRef = useRef(placed);
  placedRef.current = placed;
  useEffect(() => {
    if (!drag) return;
    const onMove = (e: PointerEvent) => {
      const d = dragRef.current;
      const el = box.current;
      if (!d || !el) return;
      const dx = e.clientX - d.x0;
      const dy = e.clientY - d.y0;
      const active = d.active || Math.hypot(dx, dy) > DRAG_START_PX;
      let over: string | null = null;
      let after = false;
      if (active) {
        const r = el.getBoundingClientRect();
        const x = e.clientX - r.left;
        const y = e.clientY - r.top;
        for (const p of placedRef.current) {
          if (p.key === d.key) continue;
          const h = heights.current.get(p.key) ?? 0;
          if (x >= p.left && x <= p.left + colWidth && y >= p.top && y <= p.top + h) {
            over = p.key;
            after = y > p.top + h / 2;
            break;
          }
        }
        document.documentElement.dataset.wallDragging = "";
      }
      setDrag({ ...d, dx, dy, active, over, after });
    };
    const onUp = () => {
      const d = dragRef.current;
      delete document.documentElement.dataset.wallDragging;
      setDrag(null);
      if (d?.active && d.over && onReorder) onReorder(d.key, d.over, d.after);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
    return () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
    };
    // 只在开始 / 结束拖动时重新挂监听，过程中的位置走 ref
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [drag?.key, colWidth, onReorder]);

  return (
    <div ref={box} className={styles.wall} style={{ height: total }}>
      {width > 0 &&
        placed.map((p) => (
          <div
            key={p.key}
            data-wall-key={p.key}
            className={styles.item}
            data-dragging={(drag?.active && drag.key === p.key) || undefined}
            data-drop={drag?.active && drag.over === p.key ? (drag.after ? "after" : "before") : undefined}
            style={{
              width: colWidth,
              transform:
                drag?.active && drag.key === p.key
                  ? `translate(${p.left + drag.dx}px, ${p.top + drag.dy}px)`
                  : `translate(${p.left}px, ${p.top}px)`,
            }}
            onPointerDown={
              onReorder
                ? (e) => {
                    if (e.button !== 0) return;
                    const t = e.target as HTMLElement;
                    // 只从标题栏拖；标题栏里的按钮（复制…）照常点
                    if (!t.closest("[data-wall-handle]") || t.closest("button, a, input, select")) return;
                    setDrag({ key: p.key, x0: e.clientX, y0: e.clientY, dx: 0, dy: 0, active: false, over: null, after: false });
                  }
                : undefined
            }
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
