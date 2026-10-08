// 自绘标题栏：系统标题栏在 Windows 10 上只能深 / 浅两种，跟不上主题色，所以主窗口去掉系统边框，
// 用主题变量画标题栏（拖动、双击最大化、最小化 / 最大化 / 关闭、侧栏开关）。

import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useState } from "react";
import { MaximizeIcon, MinimizeIcon, SidebarIcon, WindowCloseIcon } from "./icons";
import styles from "./TitleBar.module.css";

interface Props {
  navShown: boolean;
  onToggleNav: () => void;
}

export function TitleBar({ navShown, onToggleNav }: Props) {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    const w = getCurrentWindow();
    const sync = () => void w.isMaximized().then(setMaximized, () => undefined);
    sync();
    const off = w.onResized(sync);
    return () => void off.then((f) => f());
  }, []);

  const w = getCurrentWindow();
  return (
    <header className={styles.bar} data-tauri-drag-region>
      <button
        type="button"
        className={styles.navBtn}
        aria-label={navShown ? "隐藏左侧导航" : "显示左侧导航"}
        aria-pressed={navShown}
        title={`${navShown ? "隐藏" : "显示"}左侧导航（Ctrl+\\）`}
        onClick={onToggleNav}
      >
        <SidebarIcon />
      </button>
      <span className={styles.title} data-tauri-drag-region>
        词镜 Lens
      </span>
      <span className={styles.controls}>
        <button type="button" className={styles.ctrl} aria-label="最小化" onClick={() => void w.minimize()}>
          <MinimizeIcon />
        </button>
        <button type="button" className={styles.ctrl} aria-label={maximized ? "还原" : "最大化"} onClick={() => void w.toggleMaximize()}>
          <MaximizeIcon restore={maximized} />
        </button>
        <button type="button" className={`${styles.ctrl} ${styles.close}`} aria-label="关闭" onClick={() => void w.close()}>
          <WindowCloseIcon />
        </button>
      </span>
    </header>
  );
}
