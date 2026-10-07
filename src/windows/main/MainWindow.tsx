import { useEffect } from "react";
import { useSettings } from "../../stores/settings";
import { ACCENTS, THEMES } from "../../theme/themes";
import { useApplyTheme } from "../../theme/useApplyTheme";
import styles from "./MainWindow.module.css";

/** M0 空主窗口：只有标题栏和临时的主题切换（M5 设置页做好后移走）。 */
export function MainWindow() {
  const settings = useSettings((s) => s.settings);
  const error = useSettings((s) => s.error);
  const load = useSettings((s) => s.load);
  const updateAppearance = useSettings((s) => s.updateAppearance);
  const resolved = useApplyTheme();

  useEffect(() => {
    void load();
  }, [load]);

  const appearance = settings?.appearance;

  return (
    <div className={styles.shell}>
      <header className={styles.bar}>
        <span className={styles.brand}>
          词镜 <span className={styles.brandLatin}>Lens</span>
        </span>

        {appearance && (
          <div className={styles.controls}>
            <div role="group" aria-label="主题" className={styles.segmented}>
              {THEMES.map((t) => (
                <button
                  key={t.id}
                  type="button"
                  aria-pressed={appearance.theme === t.id}
                  className={styles.segment}
                  onClick={() => updateAppearance({ theme: t.id, followSystem: false })}
                >
                  {t.name}
                </button>
              ))}
            </div>

            <div role="group" aria-label="强调色" className={styles.swatches}>
              {ACCENTS.map((a) => (
                <button
                  key={a.id}
                  type="button"
                  aria-label={a.name}
                  title={a.name}
                  aria-pressed={appearance.accent === a.id}
                  data-accent={a.id}
                  data-theme={resolved?.theme}
                  className={styles.swatch}
                  onClick={() => updateAppearance({ accent: a.id })}
                />
              ))}
            </div>
          </div>
        )}
      </header>

      <main className={styles.empty}>
        <p className={styles.emptyTitle}>还没有词典</p>
        <p className={styles.emptySub}>词典导入在 M1 完成后可用。</p>
        {error && (
          <p role="alert" className={styles.error}>
            {error}
          </p>
        )}
      </main>
    </div>
  );
}
