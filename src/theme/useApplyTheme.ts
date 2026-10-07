import { useEffect, useState } from "react";
import { useSettings } from "../stores/settings";
import { applyTheme, resolveTheme, type AccentId, type ThemeId } from "./themes";

const DARK_QUERY = "(prefers-color-scheme: dark)";

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

  const resolved = appearance ? resolveTheme(appearance, systemDark) : null;
  const theme = resolved?.theme;
  const accent = resolved?.accent;

  useEffect(() => {
    if (theme && accent) applyTheme(theme, accent);
  }, [theme, accent]);

  return resolved;
}
