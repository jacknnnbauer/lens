import { useEffect, useState } from "react";
import { create } from "zustand";
import { useImmersive } from "../stores/immersive";
import { useSettings } from "../stores/settings";
import { applyTheme, findTheme, isThemeId, resolveTheme, type AccentId, type CustomTheme, type ThemeId } from "./themes";

const DARK_QUERY = "(prefers-color-scheme: dark)";

/** 当前已应用到 <html> 的主题，供词典正文等需要随主题重算的地方订阅 */
export const useAppliedTheme = create<{ theme: ThemeId | null; dark: boolean }>(() => ({ theme: null, dark: false }));

const NO_CUSTOM: CustomTheme[] = [];

/** 把设置里的主题 / 强调色同步到 <html data-theme data-accent>，并跟随系统深浅色变化。 */
export function useApplyTheme(): { theme: ThemeId; accent: AccentId } | null {
  const appearance = useSettings((s) => s.settings?.appearance);
  const [systemDark, setSystemDark] = useState(() => window.matchMedia(DARK_QUERY).matches);

  useEffect(() => {
    const mq = window.matchMedia(DARK_QUERY);
    const onChange = (e: MediaQueryListEvent) => setSystemDark(e.matches);
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);

  // 沉浸模式可单独指定主题（设置 › 沉浸模式）；只有主窗口会打开沉浸模式
  const customs = useSettings((s) => s.settings?.customThemes) ?? NO_CUSTOM;
  const immersiveTheme = useSettings((s) => s.settings?.immersive?.theme ?? null);
  const immersiveOn = useImmersive((s) => s.on);
  const override = immersiveOn && immersiveTheme && isThemeId(immersiveTheme, customs) ? immersiveTheme : null;
  const base = appearance ? resolveTheme(appearance, systemDark, customs) : null;
  const resolved = base && override ? { ...base, theme: override } : base;
  const theme = resolved?.theme;
  const accent = resolved?.accent;

  const info = findTheme(theme, customs);
  const custom = info?.custom;
  useEffect(() => {
    if (theme && accent) {
      applyTheme(theme, accent, document.documentElement, custom);
      useAppliedTheme.setState({ theme, dark: info?.dark ?? false });
    }
  }, [theme, accent, custom, info?.dark]);

  // 释义字号与字体（设置 › 外观）
  const glossSize = appearance?.glossFontSize;
  const glossFont = appearance?.glossFont;
  useEffect(() => {
    const root = document.documentElement;
    if (glossSize) root.style.setProperty("--gloss-size", `${glossSize}px`);
    if (glossFont) root.dataset.glossFont = glossFont;
  }, [glossSize, glossFont]);

  return resolved;
}
