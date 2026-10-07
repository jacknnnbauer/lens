// 主题与强调色元数据（SPEC 9.5）。颜色值只在 tokens.css 里定义。

export const THEMES = [
  { id: "paper", name: "纸本", dark: false },
  { id: "light", name: "冷白", dark: false },
  { id: "dark", name: "石墨", dark: true },
] as const;

export const ACCENTS = [
  { id: "blue", name: "靛蓝" },
  { id: "teal", name: "青绿" },
  { id: "amber", name: "琥珀" },
  { id: "violet", name: "紫罗兰" },
  { id: "rose", name: "玫红" },
] as const;

export type ThemeId = (typeof THEMES)[number]["id"];
export type AccentId = (typeof ACCENTS)[number]["id"];

export const DEFAULT_THEME: ThemeId = "paper";
export const DEFAULT_ACCENT: AccentId = "blue";

export function isThemeId(v: string): v is ThemeId {
  return THEMES.some((t) => t.id === v);
}

export function isAccentId(v: string): v is AccentId {
  return ACCENTS.some((a) => a.id === v);
}

export interface ThemeChoice {
  theme: string;
  followSystem: boolean;
  lightTheme: string;
  darkTheme: string;
  accent: string;
}

/** 根据设置和系统深浅色算出实际生效的主题；未知 id（如手改 settings.json）回退默认值。 */
export function resolveTheme(
  a: ThemeChoice,
  systemDark: boolean,
): { theme: ThemeId; accent: AccentId } {
  const wanted = a.followSystem ? (systemDark ? a.darkTheme : a.lightTheme) : a.theme;
  const fallback = a.followSystem && systemDark ? "dark" : DEFAULT_THEME;
  return {
    theme: isThemeId(wanted) ? wanted : fallback,
    accent: isAccentId(a.accent) ? a.accent : DEFAULT_ACCENT,
  };
}

export function applyTheme(theme: ThemeId, accent: AccentId, root: HTMLElement = document.documentElement) {
  root.dataset.theme = theme;
  root.dataset.accent = accent;
}
