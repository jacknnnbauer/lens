// 主题与强调色元数据（SPEC 9.5、画板 09）。内置主题的颜色只在 tokens.css 里定义；
// 自定义主题（主题库「复制并编辑」或导入）存在 settings.customThemes，由 applyTheme 写成内联变量。

export const BUILTIN_THEMES = [
  { id: "paper", name: "纸本", dark: false, accentable: true, note: "暖米白底、白卡片，宋体标题，适合长读" },
  { id: "light", name: "冷白", dark: false, accentable: true, note: "中性偏冷，干净利落，办公场景" },
  { id: "xuan", name: "宣纸", dark: false, accentable: false, note: "米黄纸色配朱砂红，东方书卷气" },
  { id: "sepia", name: "护眼棕", dark: false, accentable: false, note: "经典棕褐阅读色，长时间看不累" },
  { id: "frost", name: "霜蓝", dark: false, accentable: false, note: "浅灰蓝底配深青，冷静的工具感" },
  { id: "eink", name: "墨水屏", dark: false, accentable: false, note: "纯黑白、零彩色，适合打印和墨水屏" },
  { id: "dark", name: "石墨", dark: true, accentable: true, note: "默认暗色，低眩光，夜间与编程" },
  { id: "midnight", name: "午夜蓝", dark: true, accentable: false, note: "深海军蓝配金色，夜读的书房感" },
  { id: "moss", name: "苔原", dark: true, accentable: false, note: "深墨绿配鼠尾草绿，柔和不刺眼" },
  { id: "dusk", name: "暮紫", dark: true, accentable: false, note: "紫灰底配玫瑰粉，安静而温暖" },
  { id: "contrast", name: "高对比", dark: true, accentable: false, note: "黑底白字黄强调，弱视与强光环境" },
] as const;

/** 兼容旧名：设置页、导出等只关心内置主题的地方 */
export const THEMES = BUILTIN_THEMES;

export const ACCENTS = [
  { id: "blue", name: "靛蓝" },
  { id: "teal", name: "青绿" },
  { id: "amber", name: "琥珀" },
  { id: "violet", name: "紫罗兰" },
  { id: "rose", name: "玫红" },
] as const;

/** 主题变量（与 tokens.css、Rust themes::VARS 一致） */
export const THEME_VARS = ["bg", "panel", "raised", "line", "text", "sub", "faint", "accent", "accent-ink", "warn"] as const;
export type ThemeVar = (typeof THEME_VARS)[number];

export interface CustomTheme {
  id: string;
  name: string;
  dark: boolean;
  colors: Record<ThemeVar, string>;
}

export type BuiltinThemeId = (typeof BUILTIN_THEMES)[number]["id"];
/** 内置 id 或 `custom-…` */
export type ThemeId = string;
export type AccentId = (typeof ACCENTS)[number]["id"];

export const DEFAULT_THEME: BuiltinThemeId = "paper";
export const DEFAULT_ACCENT: AccentId = "blue";

export interface ThemeInfo {
  id: string;
  name: string;
  dark: boolean;
  accentable: boolean;
  note: string;
  custom?: CustomTheme;
}

export function isBuiltinTheme(v: string): v is BuiltinThemeId {
  return BUILTIN_THEMES.some((t) => t.id === v);
}

export function isAccentId(v: string): v is AccentId {
  return ACCENTS.some((a) => a.id === v);
}

/** 全部可选主题：内置在前，自定义在后 */
export function allThemes(customs: readonly CustomTheme[] = []): ThemeInfo[] {
  return [
    ...BUILTIN_THEMES.map((t) => ({ ...t })),
    ...customs.map((c) => ({ id: c.id, name: c.name, dark: c.dark, accentable: false, note: "", custom: c })),
  ];
}

export function findTheme(id: string | null | undefined, customs: readonly CustomTheme[] = []): ThemeInfo | undefined {
  return id ? allThemes(customs).find((t) => t.id === id) : undefined;
}

export function isThemeId(v: string, customs: readonly CustomTheme[] = []): boolean {
  return findTheme(v, customs) !== undefined;
}

export interface ThemeChoice {
  theme: string;
  followSystem: boolean;
  lightTheme: string;
  darkTheme: string;
  accent: string;
}

/** 根据设置和系统深浅色算出实际生效的主题；未知 id（如删掉的自定义主题、手改 settings.json）回退默认值。 */
export function resolveTheme(
  a: ThemeChoice,
  systemDark: boolean,
  customs: readonly CustomTheme[] = [],
): { theme: ThemeId; accent: AccentId } {
  const wanted = a.followSystem ? (systemDark ? a.darkTheme : a.lightTheme) : a.theme;
  const fallback = a.followSystem && systemDark ? "dark" : DEFAULT_THEME;
  return {
    theme: isThemeId(wanted, customs) ? wanted : fallback,
    accent: isAccentId(a.accent) ? a.accent : DEFAULT_ACCENT,
  };
}

/** 自定义主题的内联变量（也用于缩略图、导出文档） */
export function customVars(c: CustomTheme): Record<string, string> {
  const out: Record<string, string> = { colorScheme: c.dark ? "dark" : "light" };
  for (const v of THEME_VARS) out[`--${v}`] = c.colors[v];
  return out;
}

/** 自定义主题写成一段 CSS（导出的 HTML 文档里用） */
export function customCss(c: CustomTheme): string {
  const vars = THEME_VARS.map((v) => `--${v}: ${c.colors[v]};`).join(" ");
  return `:root { color-scheme: ${c.dark ? "dark" : "light"}; ${vars} }`;
}

/**
 * 应用主题。自定义主题以同深浅的内置主题为底（data-theme 决定其余非颜色变量与控件配色），
 * 颜色变量写在内联样式上，优先级高于 tokens.css 的属性选择器。
 */
export function applyTheme(theme: ThemeId, accent: AccentId, root: HTMLElement = document.documentElement, custom?: CustomTheme) {
  for (const v of THEME_VARS) root.style.removeProperty(`--${v}`);
  root.style.removeProperty("color-scheme");
  if (custom) {
    root.dataset.theme = custom.dark ? "dark" : "paper";
    root.dataset.customTheme = custom.id;
    for (const [k, v] of Object.entries(customVars(custom))) {
      if (k === "colorScheme") root.style.setProperty("color-scheme", v);
      else root.style.setProperty(k, v);
    }
  } else {
    root.dataset.theme = theme;
    delete root.dataset.customTheme;
  }
  root.dataset.accent = accent;
}

// —— 对比度（WCAG 2.x），主题编辑器与导入检查用 ——

function channel(c: number): number {
  const s = c / 255;
  return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
}

export function luminance(hex: string): number | null {
  const m = /^#([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return null;
  const n = parseInt(m[1], 16);
  return 0.2126 * channel((n >> 16) & 255) + 0.7152 * channel((n >> 8) & 255) + 0.0722 * channel(n & 255);
}

export function contrastRatio(a: string, b: string): number | null {
  const la = luminance(a);
  const lb = luminance(b);
  if (la === null || lb === null) return null;
  const [hi, lo] = la > lb ? [la, lb] : [lb, la];
  return (hi + 0.05) / (lo + 0.05);
}

/** 需要检查的配对：前景 / 背景 / 最低对比度 / 说明 */
export const CONTRAST_PAIRS: readonly { fg: ThemeVar; bg: ThemeVar; min: number; label: string }[] = [
  { fg: "text", bg: "bg", min: 7, label: "正文 / 背景" },
  { fg: "text", bg: "panel", min: 7, label: "正文 / 卡片" },
  { fg: "sub", bg: "panel", min: 4.5, label: "次要文字 / 卡片" },
  { fg: "faint", bg: "panel", min: 3, label: "提示文字 / 卡片" },
  { fg: "accent", bg: "panel", min: 3, label: "强调色 / 卡片" },
  { fg: "accent-ink", bg: "accent", min: 4.5, label: "按钮文字 / 强调色" },
  { fg: "warn", bg: "panel", min: 3, label: "警示色 / 卡片" },
];

export interface ContrastIssue {
  label: string;
  ratio: number;
  min: number;
}

export function contrastIssues(colors: Record<ThemeVar, string>): ContrastIssue[] {
  const out: ContrastIssue[] = [];
  for (const p of CONTRAST_PAIRS) {
    const r = contrastRatio(colors[p.fg], colors[p.bg]);
    if (r !== null && r < p.min) out.push({ label: p.label, ratio: r, min: p.min });
  }
  return out;
}

/** 读出某个内置主题当前的变量值（复制并编辑时用）：在一个临时元素上取计算值。 */
export function readBuiltinColors(id: BuiltinThemeId): Record<ThemeVar, string> {
  const probe = document.createElement("div");
  probe.dataset.theme = id;
  probe.style.display = "none";
  document.body.append(probe);
  const cs = getComputedStyle(probe);
  const out = {} as Record<ThemeVar, string>;
  for (const v of THEME_VARS) out[v] = toHex(cs.getPropertyValue(`--${v}`).trim());
  probe.remove();
  return out;
}

function toHex(c: string): string {
  if (/^#[0-9a-f]{6}$/i.test(c)) return c.toUpperCase();
  if (/^#[0-9a-f]{3}$/i.test(c)) return ("#" + [...c.slice(1)].map((x) => x + x).join("")).toUpperCase();
  const m = /rgba?\(\s*(\d+)[,\s]+(\d+)[,\s]+(\d+)/i.exec(c);
  if (m) return "#" + [m[1], m[2], m[3]].map((x) => Number(x).toString(16).padStart(2, "0")).join("").toUpperCase();
  return "#888888";
}
