// 主题缩略图：直接用该主题的变量渲染（data-theme 作用在缩略图元素上；自定义主题再叠内联变量）。

import type { CSSProperties } from "react";
import { tr } from "../i18n";
import { customVars, type ThemeInfo } from "../theme/themes";
import styles from "./ThemeThumb.module.css";

export function themeScope(t: ThemeInfo, accent: string): { "data-theme": string; "data-accent": string; style?: CSSProperties } {
  if (t.custom) return { "data-theme": t.dark ? "dark" : "paper", "data-accent": accent, style: customVars(t.custom) as CSSProperties };
  return { "data-theme": t.id, "data-accent": accent };
}

export function ThemeThumb({ theme, accent }: { theme: ThemeInfo; accent: string }) {
  return (
    <span className={styles.thumb} {...themeScope(theme, accent)} aria-hidden="true">
      <span className={styles.word}>{tr("词镜 Aa")}</span>
      <span className={styles.search} />
      <span className={styles.cards}>
        <i />
        <i />
      </span>
    </span>
  );
}
