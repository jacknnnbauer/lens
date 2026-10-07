import { useEffect, useState } from "react";
import { AppearanceMenu } from "../../components/AppearanceMenu";
import { LibraryIcon } from "../../components/icons";
import { LibraryDialog } from "../../components/LibraryDialog";
import { LookupPage } from "../../routes/lookup/LookupPage";
import { subscribeDictEvents, useDicts } from "../../stores/dicts";
import { subscribeLookupEvents } from "../../stores/lookup";
import { useSettings } from "../../stores/settings";
import { useApplyTheme } from "../../theme/useApplyTheme";
import styles from "./MainWindow.module.css";

/** 主窗口：目前只有查词页（简洁单栏）；其他路由随后续里程碑加入。 */
export function MainWindow() {
  const loadSettings = useSettings((s) => s.load);
  const settingsError = useSettings((s) => s.error);
  const glossFontSize = useSettings((s) => s.settings?.appearance.glossFontSize);
  const refreshDicts = useDicts((s) => s.refresh);
  const resolved = useApplyTheme();
  const [libraryOpen, setLibraryOpen] = useState(false);

  useEffect(() => {
    void loadSettings();
    void refreshDicts();
    const subs = [subscribeLookupEvents(), subscribeDictEvents()];
    return () => {
      for (const s of subs) void s.then((off) => off());
    };
  }, [loadSettings, refreshDicts]);

  useEffect(() => {
    if (glossFontSize) document.documentElement.style.setProperty("--gloss-size", `${glossFontSize}px`);
  }, [glossFontSize]);

  return (
    <div className={styles.shell}>
      {settingsError && (
        <p role="alert" className={styles.error}>
          设置读取失败：{settingsError}
        </p>
      )}
      <LookupPage
        onOpenLibrary={() => setLibraryOpen(true)}
        actions={
          <>
            <button type="button" className={styles.iconBtn} aria-label="词典库" title="词典库" onClick={() => setLibraryOpen(true)}>
              <LibraryIcon />
            </button>
            <AppearanceMenu resolvedTheme={resolved?.theme} />
          </>
        }
      />
      <LibraryDialog open={libraryOpen} onClose={() => setLibraryOpen(false)} />
    </div>
  );
}
