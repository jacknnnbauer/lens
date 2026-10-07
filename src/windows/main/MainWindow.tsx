import { useCallback, useEffect, useRef, useState } from "react";
import { AppearanceMenu } from "../../components/AppearanceMenu";
import { HotkeyNotice, useCaptureStatus } from "../../components/HotkeyNotice";
import { ContextPanel } from "../../components/ContextPanel";
import { LibraryIcon } from "../../components/icons";
import { LibraryDialog } from "../../components/LibraryDialog";
import { SideNav } from "../../components/SideNav";
import { StatusBar } from "../../components/StatusBar";
import { onMainLookup } from "../../ipc/capture";
import { LookupPage, type LookupPageHandle } from "../../routes/lookup/LookupPage";
import { subscribeDictEvents, useDicts } from "../../stores/dicts";
import { subscribeLookupEvents, useLookup } from "../../stores/lookup";
import { useSettings } from "../../stores/settings";
import { useApplyTheme } from "../../theme/useApplyTheme";
import styles from "./MainWindow.module.css";

/**
 * 主窗口。布局（SPEC 9.2）：简洁单栏 / 双栏（+左侧导航）/ 三栏（+右侧查询详情），
 * 状态栏与跳转条可单独开关。目前只有查词页；其他路由随后续里程碑加入。
 */
export function MainWindow() {
  const loadSettings = useSettings((s) => s.load);
  const settingsError = useSettings((s) => s.error);
  const glossFontSize = useSettings((s) => s.settings?.appearance.glossFontSize);
  const layout = useSettings((s) => s.settings?.layout);
  const updateNow = useSettings((s) => s.updateNow);
  const refreshDicts = useDicts((s) => s.refresh);
  const profiles = useDicts((s) => s.profiles);
  const resolved = useApplyTheme();
  const [libraryOpen, setLibraryOpen] = useState(false);
  const page = useRef<LookupPageHandle>(null);
  const capture = useCaptureStatus();

  useEffect(() => {
    void loadSettings();
    void refreshDicts();
    const subs = [
      subscribeLookupEvents(),
      subscribeDictEvents(),
      // 弹窗里按 Enter：在主窗口查同一个词
      onMainLookup((q) => page.current?.lookupWord(q)),
    ];
    return () => {
      for (const s of subs) void s.then((off) => off());
    };
  }, [loadSettings, refreshDicts]);

  useEffect(() => {
    if (glossFontSize) document.documentElement.style.setProperty("--gloss-size", `${glossFontSize}px`);
  }, [glossFontSize]);

  // 切换场景配置组：先立即保存（后端查词时读取），再用同一个词重新查
  const switchProfile = useCallback(
    async (id: string) => {
      await updateNow({ activeProfile: id });
      await useLookup.getState().rerun();
    },
    [updateNow],
  );

  // Ctrl+1…9 切换配置组，Ctrl+L 打开词典库（SPEC 9.4）
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!e.ctrlKey || e.altKey || e.shiftKey) return;
      const n = Number(e.key);
      if (n >= 1 && n <= 9 && profiles[n - 1]) {
        e.preventDefault();
        void switchProfile(profiles[n - 1].id);
      } else if (e.key.toLowerCase() === "l") {
        e.preventDefault();
        setLibraryOpen(true);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [profiles, switchProfile]);

  const showNav = layout?.showNav ?? false;
  const showContext = layout?.showContext ?? false;

  return (
    <div className={styles.shell} data-nav={showNav || undefined} data-context={showContext || undefined}>
      {showNav && (
        <div className={styles.side}>
          <SideNav
            onFocusSearch={() => page.current?.focusSearch()}
            onOpenLibrary={() => setLibraryOpen(true)}
            onSwitchProfile={(id) => void switchProfile(id)}
            onLookup={(w) => page.current?.lookupWord(w)}
          />
        </div>
      )}

      <main className={styles.main}>
        <HotkeyNotice status={capture} />
        {settingsError && (
          <p role="alert" className={styles.error}>
            设置读取失败：{settingsError}
          </p>
        )}
        <LookupPage
          ref={page}
          onOpenLibrary={() => setLibraryOpen(true)}
          showJumpBar={layout?.showJumpBar ?? true}
          showHeadword={showNav}
          expandMode={layout?.cardsExpanded ?? "first2"}
          actions={
            <>
              <button type="button" className={styles.iconBtn} aria-label="词典库" title="词典库（Ctrl+L）" onClick={() => setLibraryOpen(true)}>
                <LibraryIcon />
              </button>
              <AppearanceMenu resolvedTheme={resolved?.theme} />
            </>
          }
        />
      </main>

      {showContext && (
        <div className={`${styles.side} ${styles.context}`}>
          <ContextPanel />
        </div>
      )}

      {layout?.showStatusBar && (
        <div className={styles.status}>
          <StatusBar capture={capture} />
        </div>
      )}

      <LibraryDialog open={libraryOpen} onClose={() => setLibraryOpen(false)} />
    </div>
  );
}
