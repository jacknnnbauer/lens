import { useCallback, useEffect, useRef, useState } from "react";
import { HotkeyNotice, useCaptureStatus } from "../../components/HotkeyNotice";
import { ContextPanel } from "../../components/ContextPanel";
import { BookmarkIcon, ClockIcon, GearIcon, LibraryIcon } from "../../components/icons";
import { LibraryDialog } from "../../components/LibraryDialog";
import { SideNav } from "../../components/SideNav";
import { StatusBar } from "../../components/StatusBar";
import { Toast } from "../../components/Toast";
import { onMainLookup } from "../../ipc/capture";
import { HistoryPage } from "../../routes/history/HistoryPage";
import { LookupPage, type LookupPageHandle } from "../../routes/lookup/LookupPage";
import { SettingsPage } from "../../routes/settings/SettingsPage";
import { WordbookPage } from "../../routes/wordbook/WordbookPage";
import { subscribeDictEvents, useDicts } from "../../stores/dicts";
import { subscribeLookupEvents, useLookup } from "../../stores/lookup";
import { useRoute } from "../../stores/route";
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
  const layout = useSettings((s) => s.settings?.layout);
  const updateNow = useSettings((s) => s.updateNow);
  const refreshDicts = useDicts((s) => s.refresh);
  const profiles = useDicts((s) => s.profiles);
  useApplyTheme();
  const [libraryOpen, setLibraryOpen] = useState(false);
  const lookupPage = useRef<LookupPageHandle>(null);
  const capture = useCaptureStatus();
  const page = useRoute((s) => s.page);
  const go = useRoute((s) => s.go);
  // 从其他页面查一个词：切回查词页（查词页一直挂载，只是隐藏）
  const lookupWord = useCallback(
    (w: string) => {
      go("lookup");
      lookupPage.current?.lookupWord(w);
    },
    [go],
  );
  const focusSearch = useCallback(() => {
    go("lookup");
    requestAnimationFrame(() => lookupPage.current?.focusSearch());
  }, [go]);

  useEffect(() => {
    void loadSettings();
    void refreshDicts();
    const subs = [
      subscribeLookupEvents(),
      subscribeDictEvents(),
      // 弹窗里按 Enter：在主窗口查同一个词
      onMainLookup((q) => {
        useRoute.getState().go("lookup");
        lookupPage.current?.lookupWord(q);
      }),
    ];
    return () => {
      for (const s of subs) void s.then((off) => off());
    };
  }, [loadSettings, refreshDicts]);

  // 切换场景配置组：先立即保存（后端查词时读取），再用同一个词重新查
  const switchProfile = useCallback(
    async (id: string) => {
      await updateNow({ activeProfile: id });
      await useLookup.getState().rerun();
    },
    [updateNow],
  );

  // Ctrl+1…9 切换配置组，Ctrl+L 词典库，Ctrl+H 历史，Ctrl+B 生词本，Ctrl+, 设置，Ctrl+K 回到查词（SPEC 9.4）
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!e.ctrlKey || e.altKey || e.shiftKey) return;
      if (e.key === ",") {
        e.preventDefault();
        go("settings");
        window.scrollTo(0, 0);
        return;
      }
      const n = Number(e.key);
      if (n >= 1 && n <= 9 && profiles[n - 1]) {
        e.preventDefault();
        void switchProfile(profiles[n - 1].id);
      } else if (e.key.toLowerCase() === "l") {
        e.preventDefault();
        setLibraryOpen(true);
      } else if (e.key.toLowerCase() === "h") {
        e.preventDefault();
        go("history");
      } else if (e.key.toLowerCase() === "b") {
        e.preventDefault();
        go("wordbook");
      } else if (e.key.toLowerCase() === "k" && useRoute.getState().page !== "lookup") {
        e.preventDefault();
        focusSearch();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [profiles, switchProfile, go, focusSearch]);

  // 设置页有自己的分类导航，不显示两侧栏
  const inSettings = page === "settings";
  const showNav = (layout?.showNav ?? false) && !inSettings;
  const showContext = (layout?.showContext ?? false) && !inSettings;

  return (
    <div className={styles.shell} data-nav={showNav || undefined} data-context={showContext || undefined}>
      {showNav && (
        <div className={styles.side}>
          <SideNav
            onFocusSearch={focusSearch}
            onOpenHistory={() => go("history")}
            onOpenWordbook={() => go("wordbook")}
            onOpenLibrary={() => setLibraryOpen(true)}
            onOpenSettings={() => go("settings")}
            onSwitchProfile={(id) => void switchProfile(id)}
            onLookup={lookupWord}
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
        <div hidden={page !== "lookup"}>
          <LookupPage
            ref={lookupPage}
            onOpenLibrary={() => setLibraryOpen(true)}
            showJumpBar={layout?.showJumpBar ?? true}
            showHeadword={showNav}
            expandMode={layout?.cardsExpanded ?? "first2"}
            actions={
              <>
                <button type="button" className={styles.iconBtn} aria-label="生词本" title="生词本（Ctrl+B）" onClick={() => go("wordbook")}>
                  <BookmarkIcon />
                </button>
                <button type="button" className={styles.iconBtn} aria-label="历史" title="历史（Ctrl+H）" onClick={() => go("history")}>
                  <ClockIcon />
                </button>
                <button type="button" className={styles.iconBtn} aria-label="词典库" title="词典库（Ctrl+L）" onClick={() => setLibraryOpen(true)}>
                  <LibraryIcon />
                </button>
                <button type="button" className={styles.iconBtn} aria-label="设置" title="设置（Ctrl+,）" onClick={() => go("settings")}>
                  <GearIcon />
                </button>
              </>
            }
          />
        </div>
        {page === "history" && <HistoryPage active onLookup={lookupWord} />}
        {page === "wordbook" && <WordbookPage active onLookup={lookupWord} />}
        {inSettings && <SettingsPage onBack={focusSearch} />}
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
      <Toast />
    </div>
  );
}
