// 一次性提示：指向一个不显眼、但用了就离不开的操作（点单字查、点章节名开目录、Ctrl+\ 开导航）。
// 不加按钮，只在第一次碰到这个场景时说一句；点「知道了」或用过一次就不再出现。

import { useEffect, useLayoutEffect, useState } from "react";
import { createPortal } from "react-dom";
import { create } from "zustand";
import { useSettings } from "../stores/settings";
import styles from "./Hint.module.css";
import { tr } from "../i18n";

export type HintId = "nav" | "han-char" | "toc";

/** 同一时间只显示一条：先到先显示，这条看过或场景过去了再轮到下一条 */
const useShowing = create<{ id: HintId | null }>(() => ({ id: null }));

/** 记为已看过（点了「知道了」或用过这个操作） */
export function markHint(id: HintId) {
  const s = useSettings.getState().settings;
  if (!s || s.general.hintsSeen.includes(id)) return;
  useSettings.getState().update({ general: { hintsSeen: [...s.general.hintsSeen, id] } });
}

interface Props {
  id: HintId;
  /** 指向的元素：带 data-hint={anchor} 的元素 */
  anchor: string;
  show: boolean;
  children: string;
}

/** 显示在指向元素的下方；元素不在页面上时不显示 */
export function Hint({ id, anchor, show, children }: Props) {
  const seen = useSettings((s) => !s.settings || s.settings.general.hintsSeen.includes(id));
  const [pos, setPos] = useState<{ top: number; left: number } | null>(null);
  // 稍等一下再出现：别和页面刚画出来的内容抢眼
  const [ready, setReady] = useState(false);
  const active = show && !seen;
  const showing = useShowing((s) => s.id);

  useEffect(() => {
    if (!active) {
      setReady(false);
      return;
    }
    const t = setTimeout(() => setReady(true), 700);
    return () => clearTimeout(t);
  }, [active]);

  // 轮到自己才显示；不显示了就让出位置
  useEffect(() => {
    if (active && ready && useShowing.getState().id === null) useShowing.setState({ id });
    if ((!active || !ready) && useShowing.getState().id === id) useShowing.setState({ id: null });
  }, [active, ready, id, showing]);
  useEffect(() => () => {
    if (useShowing.getState().id === id) useShowing.setState({ id: null });
  }, [id]);

  useLayoutEffect(() => {
    if (!active || !ready) return;
    const place = () => {
      const el = document.querySelector<HTMLElement>(`[data-hint="${anchor}"]`);
      const r = el?.getBoundingClientRect();
      if (!r || r.width === 0) {
        setPos(null);
        return;
      }
      setPos({ top: r.bottom + 8, left: Math.max(8, Math.min(r.left, window.innerWidth - 296)) });
    };
    place();
    window.addEventListener("resize", place);
    window.addEventListener("scroll", place, true);
    return () => {
      window.removeEventListener("resize", place);
      window.removeEventListener("scroll", place, true);
    };
  }, [active, ready, anchor]);

  if (!active || !ready || !pos || showing !== id) return null;
  return createPortal(
    <div className={styles.hint} role="note" style={{ top: pos.top, left: pos.left }}>
      <span>{children}</span>
      <button type="button" className={styles.ok} onClick={() => markHint(id)}>
        {tr("知道了")}
      </button>
    </div>,
    document.body,
  );
}
