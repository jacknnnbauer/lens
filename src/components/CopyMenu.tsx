// 复制分裂按钮（画板 05）：主按钮按默认格式复制，下拉选格式或设默认格式。
// 菜单键盘：↑↓ 移动，Enter 选择，M/H/T 直接复制，Esc 关闭并回到触发按钮。

import { useEffect, useId, useRef, useState } from "react";
import type { CopyFormat } from "../copy/convert";
import { FORMAT_NAME } from "../copy/copy";
import styles from "./CopyMenu.module.css";
import { ChevronIcon } from "./icons";
import { tr } from "../i18n";

const FORMATS: Array<{ id: CopyFormat; key: string }> = [
  { id: "md", key: "M" },
  { id: "html", key: "H" },
  { id: "text", key: "T" },
];

interface Props {
  /** 用于无障碍名称，如「牛津高阶」 */
  label: string;
  defaultFormat: CopyFormat;
  disabled?: boolean;
  onCopy: (format: CopyFormat) => void;
  onSetDefault: (format: CopyFormat) => void;
}

export function CopyIcon({ size = 13 }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" aria-hidden="true">
      <rect x="8" y="8" width="12" height="12" rx="2" />
      <path d="M16 8V5a1 1 0 00-1-1H5a1 1 0 00-1 1v10a1 1 0 001 1h3" />
    </svg>
  );
}

export function CopyMenu({ label, defaultFormat, disabled, onCopy, onSetDefault }: Props) {
  const [open, setOpen] = useState(false);
  const wrap = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const menuId = useId();

  useEffect(() => {
    if (!open) return;
    wrap.current?.querySelector<HTMLButtonElement>("[role^=menuitem]")?.focus();
    const onDown = (e: MouseEvent) => {
      if (!wrap.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

  const close = () => {
    setOpen(false);
    trigger.current?.focus();
  };

  const onMenuKey = (e: React.KeyboardEvent) => {
    const items = Array.from(wrap.current?.querySelectorAll<HTMLButtonElement>("[role^=menuitem]") ?? []);
    const i = items.indexOf(document.activeElement as HTMLButtonElement);
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const next = (i + (e.key === "ArrowDown" ? 1 : -1) + items.length) % items.length;
      items[next]?.focus();
    } else if (e.key === "Escape" || e.key === "Tab") {
      e.preventDefault();
      e.stopPropagation();
      close();
    } else {
      const f = FORMATS.find((x) => x.key === e.key.toUpperCase());
      if (f && !e.ctrlKey && !e.altKey) {
        e.preventDefault();
        close();
        onCopy(f.id);
      }
    }
  };

  return (
    <div ref={wrap} className={styles.wrap}>
      <div className={styles.split} data-ui="split">
        <button
          type="button"
          className={styles.main}
          disabled={disabled}
          aria-label={tr("复制{label}（{format}）", { label, format: tr(FORMAT_NAME[defaultFormat]) })}
          title={tr("复制为 {format}", { format: tr(FORMAT_NAME[defaultFormat]) })}
          onClick={() => onCopy(defaultFormat)}
        >
          <CopyIcon />
          {tr("复制")}
        </button>
        <button
          ref={trigger}
          type="button"
          className={styles.more}
          disabled={disabled}
          aria-label={tr("选择{label}的复制格式", { label })}
          aria-haspopup="menu"
          aria-expanded={open}
          aria-controls={open ? menuId : undefined}
          onClick={() => setOpen((o) => !o)}
        >
          <ChevronIcon size={12} open={open} />
        </button>
      </div>
      {open && (
        <div id={menuId} role="menu" aria-label={tr("复制格式")} className={styles.menu} onKeyDown={onMenuKey}>
          {FORMATS.map((f) => (
            <button
              key={f.id}
              type="button"
              role="menuitem"
              tabIndex={-1}
              className={styles.item}
              onClick={() => {
                close();
                onCopy(f.id);
              }}
            >
              {tr("复制为 {format}", { format: tr(FORMAT_NAME[f.id]) })}
              <kbd>{f.key}</kbd>
            </button>
          ))}
          <div className={styles.sep} role="separator" />
          <div className={styles.groupLabel}>{tr("默认格式")}</div>
          {FORMATS.map((f) => (
            <button
              key={`d-${f.id}`}
              type="button"
              role="menuitemradio"
              aria-checked={defaultFormat === f.id}
              tabIndex={-1}
              className={styles.item}
              onClick={() => {
                onSetDefault(f.id);
                close();
              }}
            >
              {FORMAT_NAME[f.id]}
              {defaultFormat === f.id && <span aria-hidden="true">✓</span>}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
