// 外观弹出菜单：主题、强调色、布局。M5 设置页完成后并入设置。

import { useEffect, useRef, useState } from "react";
import type { Layout, LayoutPreset } from "../ipc/settings";
import { useSettings } from "../stores/settings";
import { ACCENTS, THEMES, type ThemeId } from "../theme/themes";
import styles from "./AppearanceMenu.module.css";
import { GearIcon } from "./icons";

const PRESETS: Array<{ id: LayoutPreset; name: string; regions: Partial<Layout> }> = [
  { id: "compact", name: "单栏", regions: { showNav: false, showContext: false, showStatusBar: false } },
  { id: "two-column", name: "双栏", regions: { showNav: true, showContext: false, showStatusBar: true } },
  { id: "three-column", name: "三栏", regions: { showNav: true, showContext: true, showStatusBar: true } },
];

export function AppearanceMenu({ resolvedTheme }: { resolvedTheme: ThemeId | undefined }) {
  const appearance = useSettings((s) => s.settings?.appearance);
  const updateAppearance = useSettings((s) => s.updateAppearance);
  const layout = useSettings((s) => s.settings?.layout);
  const updateLayout = useSettings((s) => s.updateLayout);
  const [open, setOpen] = useState(false);
  const wrap = useRef<HTMLDivElement>(null);
  const button = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (!wrap.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setOpen(false);
        button.current?.focus();
      }
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div ref={wrap} className={styles.wrap}>
      <button
        ref={button}
        type="button"
        className={styles.trigger}
        aria-label="外观"
        aria-expanded={open}
        aria-haspopup="true"
        onClick={() => setOpen((o) => !o)}
      >
        <GearIcon />
      </button>
      {open && appearance && (
        <div className={styles.panel} role="group" aria-label="外观设置">
          <div className={styles.label}>主题</div>
          <div role="group" aria-label="主题" className={styles.segmented}>
            {THEMES.map((t) => (
              <button
                key={t.id}
                type="button"
                aria-pressed={appearance.theme === t.id && !appearance.followSystem}
                className={styles.segment}
                onClick={() => updateAppearance({ theme: t.id, followSystem: false })}
              >
                {t.name}
              </button>
            ))}
          </div>
          <label className={styles.check}>
            <input
              type="checkbox"
              checked={appearance.followSystem}
              onChange={(e) => updateAppearance({ followSystem: e.target.checked })}
            />
            跟随系统深浅色
          </label>
          {layout && (
            <>
              <div className={styles.label}>布局</div>
              <div role="group" aria-label="布局" className={styles.segmented}>
                {PRESETS.map((p) => (
                  <button
                    key={p.id}
                    type="button"
                    aria-pressed={layout.preset === p.id}
                    className={styles.segment}
                    onClick={() => updateLayout({ preset: p.id, ...p.regions })}
                  >
                    {p.name}
                  </button>
                ))}
              </div>
              <label className={styles.check}>
                <input type="checkbox" checked={layout.showJumpBar} onChange={(e) => updateLayout({ showJumpBar: e.target.checked })} />
                词典跳转条
              </label>
              <label className={styles.check}>
                <input type="checkbox" checked={layout.showStatusBar} onChange={(e) => updateLayout({ showStatusBar: e.target.checked })} />
                状态栏
              </label>
            </>
          )}
          <div className={styles.label}>强调色</div>
          <div role="group" aria-label="强调色" className={styles.swatches}>
            {ACCENTS.map((a) => (
              <button
                key={a.id}
                type="button"
                aria-label={a.name}
                title={a.name}
                aria-pressed={appearance.accent === a.id}
                data-accent={a.id}
                data-theme={resolvedTheme}
                className={styles.swatch}
                onClick={() => updateAppearance({ accent: a.id })}
              />
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
