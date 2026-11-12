import { tr } from "../../i18n";

// 录制全局快捷键：把一次按键转成 tauri-plugin-global-shortcut 能解析的写法（如 "Ctrl+Alt+L"）。

export interface KeyLike {
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
  code: string;
}

const MODIFIER_CODES = /^(Control|Alt|Shift|Meta|OS)(Left|Right)?$/;
/** 系统或输入法占用、注册了也收不到的组合 */
const RESERVED = new Set(["Alt+Space", "Ctrl+Space", "Ctrl+Shift+Space", "Alt+Tab", "Alt+F4", "Ctrl+Alt+Delete"]);

export function keyName(code: string): string | null {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit\d$/.test(code)) return code.slice(5);
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) return code;
  const named: Record<string, string> = {
    Space: "Space",
    Backquote: "Backquote",
    Minus: "Minus",
    Equal: "Equal",
    BracketLeft: "BracketLeft",
    BracketRight: "BracketRight",
    Backslash: "Backslash",
    Semicolon: "Semicolon",
    Quote: "Quote",
    Comma: "Comma",
    Period: "Period",
    Slash: "Slash",
    Insert: "Insert",
    Home: "Home",
    End: "End",
    PageUp: "PageUp",
    PageDown: "PageDown",
  };
  return named[code] ?? null;
}

export type HotkeyResult = { ok: true; value: string } | { ok: false; reason: string } | null;

/** 只按了修饰键时返回 null（继续等）；组合无效时返回原因。 */
export function formatHotkey(e: KeyLike): HotkeyResult {
  if (MODIFIER_CODES.test(e.code)) return null;
  const key = keyName(e.code);
  if (!key) return { ok: false, reason: tr("这个键不能用作全局快捷键") };
  const mods = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Super"].filter(Boolean) as string[];
  const isF = /^F\d+$/.test(key);
  // 单独的字母、数字会吞掉正常输入；至少要有 Ctrl / Alt / Win 之一（功能键可以单独用）
  if (!isF && !(e.ctrlKey || e.altKey || e.metaKey)) return { ok: false, reason: tr("需要搭配 Ctrl、Alt 或 Win") };
  const value = [...mods, key].join("+");
  if (RESERVED.has(value)) return { ok: false, reason: tr("{combo} 被系统或输入法占用", { combo: value }) };
  return { ok: true, value };
}
