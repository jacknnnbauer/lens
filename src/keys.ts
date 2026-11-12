// 应用内快捷键（SPEC 9.4）：可在 设置 › 快捷键 里改。设置里只存改过的（settings.keys：动作 id → 组合键），
// 没改的用默认值。组合键写法与全局快捷键相同（Ctrl+Shift+C、Ctrl+Comma），见 routes/settings/hotkey.ts。

import { keyName } from "./routes/settings/hotkey";
import { useSettings } from "./stores/settings";

export type KeyAction =
  | "focusSearch"
  | "save"
  | "copyCurrent"
  | "copyAll"
  | "export"
  | "wordbook"
  | "history"
  | "library"
  | "fulltext"
  | "toggleNav"
  | "toggleContext"
  | "cardWall"
  | "settings"
  | "popupPin"
  | "bookmark";

/** 可自定义的动作：id、默认组合键、说明（中文，界面上再翻译） */
export const KEY_ACTIONS: ReadonlyArray<{ id: KeyAction; def: string; label: string }> = [
  { id: "focusSearch", def: "Ctrl+K", label: "聚焦搜索框" },
  { id: "save", def: "Ctrl+S", label: "收藏（带语境句；主窗口与查词小窗）" },
  { id: "copyCurrent", def: "Ctrl+Shift+C", label: "复制当前词典（主窗口与查词小窗）" },
  { id: "copyAll", def: "Ctrl+Shift+A", label: "合并复制所有词典" },
  { id: "export", def: "Ctrl+E", label: "导出当前词" },
  { id: "wordbook", def: "Ctrl+B", label: "生词本" },
  { id: "history", def: "Ctrl+H", label: "历史" },
  { id: "library", def: "Ctrl+L", label: "词典库" },
  { id: "fulltext", def: "Ctrl+Shift+F", label: "反查与全文搜索" },
  { id: "toggleNav", def: "Ctrl+Backslash", label: "显示 / 隐藏左侧导航" },
  { id: "toggleContext", def: "Ctrl+Shift+Backslash", label: "显示 / 隐藏右侧上下文栏" },
  { id: "cardWall", def: "Ctrl+Shift+W", label: "卡片墙 开 / 关" },
  { id: "settings", def: "Ctrl+Comma", label: "设置" },
  { id: "popupPin", def: "Ctrl+P", label: "查词小窗：钉住（失焦不自动关闭）" },
  { id: "bookmark", def: "Ctrl+D", label: "阅读：在当前位置加书签" },
];

const DEFAULTS = Object.fromEntries(KEY_ACTIONS.map((a) => [a.id, a.def])) as Record<KeyAction, string>;

/** 按下的组合键，写法与设置里存的一致；只按了修饰键或不支持的键返回 null */
export function comboOf(e: Pick<KeyboardEvent, "ctrlKey" | "altKey" | "shiftKey" | "metaKey" | "code">): string | null {
  const key = keyName(e.code);
  if (!key) return null;
  const mods = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Super"].filter(Boolean) as string[];
  return [...mods, key].join("+");
}

/** 当前生效的组合键（keys 为设置里的覆盖值） */
export function keyFor(id: KeyAction, keys?: Record<string, string> | null): string {
  const k = keys?.[id];
  return typeof k === "string" && k ? k : DEFAULTS[id];
}

/** 这次按键是不是某个动作（事件发生时读设置，监听器不必随设置重建） */
export function pressed(e: KeyboardEvent | React.KeyboardEvent, id: KeyAction): boolean {
  const combo = comboOf(e);
  return combo !== null && combo === keyFor(id, useSettings.getState().settings?.keys);
}

const SHOW: Record<string, string> = {
  Comma: ",",
  Period: ".",
  Slash: "/",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  Backquote: "`",
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Super: "Win",
};

/** 显示用：Ctrl+Comma → Ctrl+, */
export function showCombo(combo: string): string {
  return combo
    .split("+")
    .map((p) => SHOW[p] ?? p)
    .join("+");
}

/** 组件里显示某个动作当前的组合键（随设置更新） */
export function useKeyLabel(id: KeyAction): string {
  const keys = useSettings((s) => s.settings?.keys);
  return showCombo(keyFor(id, keys));
}

/** 改了会妨碍正常编辑或系统功能的组合 */
const BLOCKED = new Set(["Ctrl+C", "Ctrl+V", "Ctrl+X", "Ctrl+A", "Ctrl+Z", "Ctrl+Y", "Alt+F4", "Alt+Tab", "Ctrl+Tab", "Ctrl+Space", "Alt+Space", "F11", "F5"]);
/** 固定不可改、但会和自定义冲突的：Ctrl+1…9 切换配置组 */
const FIXED = /^Ctrl\+[1-9]$/;

export type KeyCheck = { ok: true; value: string } | { ok: false; reason: string } | null;

/**
 * 为动作 id 录到的一次按键：只按了修饰键返回 null（继续等），否则给出能否使用。
 * 冲突检查：其他动作当前的组合、配置组 Ctrl+1…9、全局划词快捷键。
 */
export function checkAppKey(
  e: Pick<KeyboardEvent, "ctrlKey" | "altKey" | "shiftKey" | "metaKey" | "code">,
  id: KeyAction,
  keys: Record<string, string> | null | undefined,
  globalHotkey: string,
  t: (s: string, v?: Record<string, string>) => string,
): KeyCheck {
  if (/^(Control|Alt|Shift|Meta|OS)(Left|Right)?$/.test(e.code)) return null;
  const combo = comboOf(e);
  if (!combo) return { ok: false, reason: t("这个键不能用作快捷键") };
  const isF = /^F\d+$/.test(keyName(e.code) ?? "");
  if (!isF && !(e.ctrlKey || e.altKey || e.metaKey)) return { ok: false, reason: t("需要搭配 Ctrl、Alt 或 Win") };
  if (BLOCKED.has(combo) || FIXED.test(combo)) return { ok: false, reason: t("{combo} 是系统或 Lens 固定使用的组合", { combo: showCombo(combo) }) };
  if (combo === globalHotkey) return { ok: false, reason: t("{combo} 已是全局划词快捷键", { combo: showCombo(combo) }) };
  const other = KEY_ACTIONS.find((a) => a.id !== id && keyFor(a.id, keys) === combo);
  if (other) return { ok: false, reason: t("{combo} 已用于「{what}」", { combo: showCombo(combo), what: t(other.label) }) };
  return { ok: true, value: combo };
}
