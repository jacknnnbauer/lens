// 沉浸模式（SPEC 9.2、画板 12）：隐藏标题栏等界面（由主窗口按设置处理）、可选全屏、F11 / Esc、J / K 翻词典，
// 以及鼠标停在顶部 48px 内 300ms 浮现、离开 1.5 秒隐藏的工具栏。

import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useRef, useState } from "react";
import { useImmersive } from "../stores/immersive";
import { useSettings } from "../stores/settings";
import styles from "./ImmersiveBar.module.css";

const REVEAL_MS = 300;
const HIDE_MS = 1500;
const HINT_MS = 4000;

function isEditable(t: EventTarget | null): boolean {
  return t instanceof HTMLElement && (t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName));
}

/** J / K：滚到下一本 / 上一本词典的标题处。以焦点所在的卡片为准（滚到底后位置不再变化），没有焦点时按位置找。 */
function stepCard(dir: 1 | -1) {
  const cards = Array.from(document.querySelectorAll<HTMLElement>("article[id^='dict-']"));
  const focused = cards.findIndex((c) => c.contains(document.activeElement));
  let index: number;
  if (focused >= 0) index = focused + dir;
  else {
    // 卡片 scroll-margin-top 为 16px：当前卡片停在 16 附近
    const tops = cards.map((c) => c.getBoundingClientRect().top);
    index = dir === 1 ? tops.findIndex((t) => t > 24) : tops.map((t) => t < 8).lastIndexOf(true);
  }
  const target = cards[index];
  if (!target) return;
  target.scrollIntoView({ block: "start", behavior: "smooth" });
  target.querySelector<HTMLElement>("[data-card-toggle]")?.focus({ preventScroll: true });
}

/** 主窗口调用一次：把开关状态落实到 <html> 标记、窗口外观与快捷键上。 */
export function useImmersiveMode() {
  const on = useImmersive((s) => s.on);
  const prefs = useSettings((s) => s.settings?.immersive);
  const started = useRef(false);

  // 启动时直接进入
  useEffect(() => {
    if (!prefs || started.current) return;
    started.current = true;
    if (prefs.onStartup) useImmersive.getState().enter();
  }, [prefs]);

  const cards = prefs?.hideCardChrome ?? true;
  const width = prefs?.width ?? 720;
  useEffect(() => {
    const root = document.documentElement;
    if (on) {
      root.dataset.immersive = "on";
      if (cards) root.dataset.immersiveCards = "bare";
      else delete root.dataset.immersiveCards;
      root.style.setProperty("--immersive-width", `${width}px`);
    } else {
      delete root.dataset.immersive;
      delete root.dataset.immersiveCards;
      root.style.removeProperty("--immersive-width");
    }
  }, [on, cards, width]);

  // 全屏；退出时恢复。主窗口没有系统标题栏（自绘），隐藏标题栏由主窗口处理。只在开关变化后改窗口，启动时不碰
  const fullscreen = !!prefs?.fullscreen;
  const touched = useRef(false);
  useEffect(() => {
    if (!on && !touched.current) return;
    touched.current = true;
    void getCurrentWindow()
      .setFullscreen(on && fullscreen)
      .catch(() => undefined);
  }, [on, fullscreen]);

  // F11 进入 / 退出；Esc 退出（对话框、菜单、搜索框先处理自己的 Esc）；J / K 翻词典
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const { on: active, toggle, exit } = useImmersive.getState();
      if (e.key === "F11" && !e.ctrlKey && !e.altKey && !e.shiftKey) {
        e.preventDefault();
        toggle();
        return;
      }
      if (!active || e.defaultPrevented || e.ctrlKey || e.altKey || e.metaKey) return;
      if (e.key === "Escape") {
        if (document.querySelector("dialog[open]")) return;
        e.preventDefault();
        exit();
      } else if ((e.key === "j" || e.key === "k") && !e.shiftKey && !isEditable(e.target)) {
        e.preventDefault();
        stepCard(e.key === "j" ? 1 : -1);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return on;
}

interface BarProps {
  revealOnHover: boolean;
  onWordbook: () => void;
  onExport: () => void;
  onSettings: () => void;
}

export function ImmersiveBar({ revealOnHover, onWordbook, onExport, onSettings }: BarProps) {
  const exit = useImmersive((s) => s.exit);
  const [shown, setShown] = useState(false);
  const [hint, setHint] = useState(true);
  const revealTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const hideTimer = useRef<ReturnType<typeof setTimeout>>(undefined);

  useEffect(() => {
    const t = setTimeout(() => setHint(false), HINT_MS);
    return () => {
      clearTimeout(t);
      clearTimeout(revealTimer.current);
      clearTimeout(hideTimer.current);
    };
  }, []);

  const enter = () => {
    clearTimeout(hideTimer.current);
    if (!revealOnHover || shown) return;
    revealTimer.current = setTimeout(() => setShown(true), REVEAL_MS);
  };
  const leave = () => {
    clearTimeout(revealTimer.current);
    clearTimeout(hideTimer.current);
    hideTimer.current = setTimeout(() => setShown(false), HIDE_MS);
  };

  return (
    <>
      {/* 顶部 48px：悬停浮现工具栏；无边框时也是拖动窗口的区域 */}
      <div className={styles.zone} data-tauri-drag-region onMouseEnter={enter} onMouseLeave={leave}>
        <div role="toolbar" aria-label="沉浸模式工具栏" className={styles.bar} data-shown={shown || undefined} onMouseEnter={enter} onMouseLeave={leave}>
          <button type="button" onClick={exit}>
            退出沉浸
          </button>
          <button
            type="button"
            onClick={() => {
              exit();
              onWordbook();
            }}
          >
            生词本
          </button>
          <button type="button" onClick={onExport}>
            导出
          </button>
          <button
            type="button"
            onClick={() => {
              exit();
              onSettings();
            }}
          >
            设置
          </button>
          <span className={styles.note}>鼠标离开后 1.5 秒自动隐藏</span>
        </div>
      </div>
      <p className={styles.hint} data-hidden={!hint || undefined} aria-hidden={!hint}>
        F11 / Esc 退出沉浸 · {revealOnHover ? "鼠标移到顶部显示工具栏 · " : ""}J / K 上下翻词典
      </p>
    </>
  );
}
