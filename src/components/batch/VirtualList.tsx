// 定高行的虚拟滚动列表：几千行也只渲染可见的几十行（SPEC 12.3「前端列表虚拟滚动」）。

import { useEffect, useRef, useState, type ReactNode } from "react";

interface Props<T> {
  items: T[];
  rowHeight: number;
  render: (item: T, index: number) => ReactNode;
  keyOf: (item: T, index: number) => string | number;
  className?: string;
  /** 可见区上下多渲染的行数 */
  overscan?: number;
  label?: string;
}

export function VirtualList<T>({ items, rowHeight, render, keyOf, className, overscan = 8, label }: Props<T>) {
  const box = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [height, setHeight] = useState(400);

  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setHeight(el.clientHeight));
    ro.observe(el);
    setHeight(el.clientHeight);
    return () => ro.disconnect();
  }, []);

  const first = Math.max(0, Math.floor(scrollTop / rowHeight) - overscan);
  const last = Math.min(items.length, Math.ceil((scrollTop + height) / rowHeight) + overscan);
  const slice = items.slice(first, last);

  return (
    <div ref={box} className={className} style={{ overflowY: "auto", position: "relative" }} onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)} role="list" aria-label={label}>
      <div style={{ height: items.length * rowHeight, position: "relative" }}>
        <div style={{ position: "absolute", top: first * rowHeight, left: 0, right: 0 }}>
          {slice.map((item, i) => (
            <div key={keyOf(item, first + i)} role="listitem" style={{ height: rowHeight }}>
              {render(item, first + i)}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
