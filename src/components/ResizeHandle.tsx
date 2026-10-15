// 侧栏宽度拖动条（三栏 / 双栏）：拖动调宽，← / → 每次 16px，双击恢复默认。
// 拖动中只改 CSS 变量（不写设置），松手才保存，避免每一帧都写 settings.json。

import { useRef } from "react";
import styles from "./ResizeHandle.module.css";

interface Props {
  label: string;
  /** 侧栏在哪一边：left = 左侧导航（向右拖变宽），right = 右侧栏（向左拖变宽） */
  side: "left" | "right";
  width: number;
  min: number;
  max: () => number;
  defaultWidth: number;
  /** 拖动中的实时宽度 */
  onPreview: (w: number) => void;
  onCommit: (w: number) => void;
}

const STEP = 16;

export function ResizeHandle({ label, side, width, min, max, defaultWidth, onPreview, onCommit }: Props) {
  const drag = useRef<{ x: number; w: number; cur: number } | null>(null);
  const clamp = (w: number) => Math.round(Math.min(Math.max(w, min), Math.max(min, max())));

  return (
    <button
      type="button"
      className={styles.handle}
      data-side={side}
      aria-label={label}
      title={label}
      onPointerDown={(e) => {
        if (e.button !== 0) return;
        e.preventDefault();
        e.currentTarget.setPointerCapture(e.pointerId);
        drag.current = { x: e.clientX, w: width, cur: width };
        document.documentElement.dataset.resizing = "";
      }}
      onPointerMove={(e) => {
        const d = drag.current;
        if (!d) return;
        const delta = (e.clientX - d.x) * (side === "left" ? 1 : -1);
        d.cur = clamp(d.w + delta);
        onPreview(d.cur);
      }}
      onPointerUp={(e) => {
        const d = drag.current;
        drag.current = null;
        delete document.documentElement.dataset.resizing;
        if (e.currentTarget.hasPointerCapture(e.pointerId)) e.currentTarget.releasePointerCapture(e.pointerId);
        if (d && d.cur !== d.w) onCommit(d.cur);
      }}
      onDoubleClick={() => {
        onPreview(defaultWidth);
        onCommit(defaultWidth);
      }}
      onKeyDown={(e) => {
        if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
        e.preventDefault();
        const grow = (e.key === "ArrowRight") === (side === "left");
        const w = clamp(width + (grow ? STEP : -STEP));
        onPreview(w);
        onCommit(w);
      }}
    />
  );
}
