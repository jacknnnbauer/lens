import { useCallback, useEffect, useRef, useState } from "react";
import { checkUpdate } from "../../ipc/settings";
import { toast } from "../../stores/toast";
import { TrayHint } from "../../components/TrayHint";
import { useNavHistory } from "./navHistory";
import { ResizeHandle } from "../../components/ResizeHandle";
import { useInAppLookup } from "../../components/inAppLookup";
import { BatchChip, BatchDialog, BatchResumeNotice } from "../../components/batch/BatchDialog";
import { HotkeyNotice, useCaptureStatus } from "../../components/HotkeyNotice";
import { ImmersiveBar, useImmersiveMode } from "../../components/ImmersiveBar";
import { ContextPanel } from "../../components/ContextPanel";
import { BookmarkIcon, ClockIcon, GearIcon, ImmersiveIcon, LibraryIcon } from "../../components/icons";
import { TitleBar } from "../../components/TitleBar";
import { useImmersive } from "../../stores/immersive";
import { SCROLL_ID, scrollRoot } from "./scroll";
import { LibraryDialog } from "../../components/LibraryDialog";
import { SideNav } from "../../components/SideNav";
import { StatusBar } from "../../components/StatusBar";
import { Toast } from "../../components/Toast";
import { onMainLookup } from "../../ipc/capture";
import { HistoryPage } from "../../routes/history/HistoryPage";
import { LookupPage, type LookupPageHandle } from "../../routes/lookup/LookupPage";
import { SettingsPage } from "../../routes/settings/SettingsPage";
import { ThemeLibraryPage } from "../../routes/themes/ThemeLibraryPage";
import { WordbookPage } from "../../routes/wordbook/WordbookPage";
import { subscribeBatchEvents, useBatch } from "../../stores/batch";
import { subscribeDictEvents, useDicts } from "../../stores/dicts";
import { subscribeLookupEvents, useLookup } from "../../stores/lookup";
import { useRoute } from "../../stores/route";
import { useSettings } from "../../stores/settings";
import { useApplyTheme } from "../../theme/useApplyTheme";
import styles from "./MainWindow.module.css";
import { tr } from "../../i18n";

/**
 * 主窗口。布局（SPEC 9.2）：简洁单栏 / 双栏（+左侧导航）/ 三栏（+右侧查询详情），
 * 状态栏与跳转条可单独开关。目前只有查词页；其他路由随后续里程碑加入。
 */
/** 侧栏宽度范围（px） */
const NAV_DEFAULT = 220;
const NAV_MIN = 160;
const CONTEXT_DEFAULT = 280;
const CONTEXT_MIN = 220;
/** 中间主区至少留这么宽 */
const MAIN_MIN = 420;

export function MainWindow() {
  const loadSettings = useSettings((s) => s.load);
  const settingsError = useSettings((s) => s.error);
  const layout = useSettings((s) => s.settings?.layout);
  const updateNow = useSettings((s) => s.updateNow);
  const refreshDicts = useDicts((s) => s.refresh);
  const profiles = useDicts((s) => s.profiles);
  useApplyTheme();
  const immersive = useImmersiveMode();
  const immersivePrefs = useSettings((s) => s.settings?.immersive);
  const [libraryOpen, setLibraryOpen] = useState(false);
  const lookupPage = useRef<LookupPageHandle>(null);
  const capture = useCaptureStatus();
  const page = useRoute((s) => s.page);
  const go = useRoute((s) => s.go);
  // 从其他页面查一个词：切回查词页（查词页一直挂载，只是隐藏）
  const lookupWord = useCallback(
    (w: string, contextId?: number | null) => {
      go("lookup");
      lookupPage.current?.lookupWord(w, contextId);
    },
    [go],
  );
  // 主窗口里划词：直接在主窗口查这个词（不弹小窗）
  useInAppLookup(lookupWord);
  // 鼠标侧键 / Alt+←→：后退、前进
  useNavHistory(lookupWord);
  const focusSearch = useCallback(() => {
    go("lookup");
    requestAnimationFrame(() => lookupPage.current?.focusSearch());
  }, [go]);

  useEffect(() => {
    void loadSettings();
    void useBatch.getState().refreshUnfinished();
    void refreshDicts();
    const subs = [
      subscribeLookupEvents(),
      subscribeDictEvents(),
      subscribeBatchEvents(),
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

  // 启动时检查新版本（设置 › 常规，每天最多一次；失败不打扰）
  useEffect(() => {
    const t = setTimeout(() => {
      checkUpdate(true).then(
        (u) => u?.newer && toast(tr("Lens 有新版本 {v}", { v: u.latest ?? "" }), tr("设置 › 关于 › 去下载")),
        () => undefined,
      );
    }, 5000);
    return () => clearTimeout(t);
  }, []);

  // 启动时回到简洁单栏（设置 › 布局，默认开）
  const startApplied = useRef(false);
  useEffect(() => {
    if (!layout || startApplied.current) return;
    startApplied.current = true;
    if (layout.startCompact) {
      useSettings.getState().updateLayout({ preset: "compact", showNav: false, showContext: false, showStatusBar: false });
    }
  }, [layout]);

  // 标题栏按钮 / Ctrl+\：随时显示或隐藏左侧导航
  const toggleNav = useCallback(() => {
    const cur = useSettings.getState().settings?.layout.showNav ?? false;
    useSettings.getState().updateLayout({ showNav: !cur });
  }, []);
  // 标题栏右侧按钮 / Ctrl+Shift+\：右侧上下文栏
  const toggleContext = useCallback(() => {
    const cur = useSettings.getState().settings?.layout.showContext ?? false;
    useSettings.getState().updateLayout({ showContext: !cur });
  }, []);

  // Ctrl+1…9 切换配置组，Ctrl+L 词典库，Ctrl+H 历史，Ctrl+B 生词本，Ctrl+, 设置，Ctrl+K 回到查词（SPEC 9.4）
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.shiftKey && !e.altKey && e.code === "Backslash") {
        // Ctrl+Shift+\ 显示 / 隐藏右侧上下文栏
        e.preventDefault();
        toggleContext();
        return;
      }
      if (!e.ctrlKey || e.altKey || e.shiftKey) return;
      if (e.key === ",") {
        e.preventDefault();
        go("settings");
        scrollRoot().scrollTo(0, 0);
        return;
      }
      if (e.key === "\\") {
        // Ctrl+\ 显示 / 隐藏左侧导航
        e.preventDefault();
        toggleNav();
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
  }, [profiles, switchProfile, go, focusSearch, toggleNav, toggleContext]);

  // 进入沉浸模式时回到查词页
  useEffect(() => {
    if (immersive) go("lookup");
  }, [immersive, go]);

  // 侧栏宽度：拖动中用本地值实时预览，松手写回设置
  const [navW, setNavW] = useState<number | null>(null);
  const [ctxW, setCtxW] = useState<number | null>(null);
  const navWidth = navW ?? layout?.navWidth ?? NAV_DEFAULT;
  const contextWidth = ctxW ?? layout?.contextWidth ?? CONTEXT_DEFAULT;
  // 中间至少留 MAIN_MIN；另一侧栏也算进去
  const maxSide = (other: number) => () => Math.max(0, window.innerWidth - MAIN_MIN - other);

  // 设置页有自己的分类导航，不显示两侧栏；沉浸模式按设置隐藏
  const inSettings = page === "settings" || page === "themes";
  const hideSides = inSettings || (immersive && (immersivePrefs?.hideSidebars ?? true));
  const showNav = (layout?.showNav ?? false) && !hideSides;
  const showContext = (layout?.showContext ?? false) && !hideSides;
  const showStatus = (layout?.showStatusBar ?? false) && !(immersive && (immersivePrefs?.hideStatusBar ?? true));
  const showTitleBar = !(immersive && (immersivePrefs?.hideTitleBar ?? true));

  return (
    <div className={styles.frame} data-titlebar={showTitleBar || undefined}>
    {showTitleBar && (
      <TitleBar navShown={layout?.showNav ?? false} onToggleNav={toggleNav} contextShown={layout?.showContext ?? false} onToggleContext={toggleContext} />
    )}
    <div id={SCROLL_ID} className={styles.scroller}>
    <div
      className={styles.shell}
      data-nav={showNav || undefined}
      data-context={showContext || undefined}
      style={{ "--nav-w": `${navWidth}px`, "--context-w": `${contextWidth}px` } as React.CSSProperties}
    >
      {showNav && (
        <div className={styles.side}>
          <ResizeHandle
            side="left"
            label={tr("拖动调整左侧导航宽度（双击恢复默认）")}
            width={navWidth}
            min={NAV_MIN}
            max={maxSide(showContext ? contextWidth : 0)}
            defaultWidth={NAV_DEFAULT}
            onPreview={setNavW}
            onCommit={(w) => {
              useSettings.getState().updateLayout({ navWidth: w });
              setNavW(null);
            }}
          />
          <SideNav
            onFocusSearch={focusSearch}
            onOpenHistory={() => go("history")}
            onOpenWordbook={() => go("wordbook")}
            onOpenLibrary={() => setLibraryOpen(true)}
            onOpenSettings={() => go("settings")}
            onOpenBatch={() => useBatch.getState().openDialog()}
            onSwitchProfile={(id) => void switchProfile(id)}
            onLookup={lookupWord}
          />
        </div>
      )}

      <main className={styles.main}>
        {!immersive && <HotkeyNotice status={capture} />}
        {!immersive && <BatchResumeNotice />}
        {settingsError && (
          <p role="alert" className={styles.error}>
            {tr("设置读取失败：{error}", { error: settingsError })}
          </p>
        )}
        <div hidden={page !== "lookup"}>
          <LookupPage
            ref={lookupPage}
            onOpenLibrary={() => setLibraryOpen(true)}
            showJumpBar={layout?.showJumpBar ?? true}
            showHeadword={showNav && !immersive}
            actions={
              <>
                <button type="button" className={styles.iconBtn} aria-label={tr("生词本")} title={tr("生词本（Ctrl+B）")} onClick={() => go("wordbook")}>
                  <BookmarkIcon />
                </button>
                <button type="button" className={styles.iconBtn} aria-label={tr("历史")} title={tr("历史（Ctrl+H）")} onClick={() => go("history")}>
                  <ClockIcon />
                </button>
                <button type="button" className={styles.iconBtn} aria-label={tr("词典库")} title={tr("词典库（Ctrl+L）")} onClick={() => setLibraryOpen(true)}>
                  <LibraryIcon />
                </button>
                <button type="button" className={styles.iconBtn} aria-label={tr("设置")} title={tr("设置（Ctrl+,）")} onClick={() => go("settings")}>
                  <GearIcon />
                </button>
                <button type="button" className={styles.iconBtn} aria-label={tr("沉浸模式")} title={tr("沉浸模式（F11）")} onClick={() => useImmersive.getState().enter()}>
                  <ImmersiveIcon />
                </button>
              </>
            }
          />
        </div>
        {page === "history" && <HistoryPage active onLookup={lookupWord} />}
        {page === "wordbook" && <WordbookPage active onLookup={lookupWord} />}
        {page === "settings" && <SettingsPage onBack={focusSearch} onOpenThemes={() => go("themes")} />}
        {page === "themes" && <ThemeLibraryPage onBack={() => go("settings")} />}
      </main>

      {showContext && (
        <div className={`${styles.side} ${styles.context}`}>
          <ResizeHandle
            side="right"
            label={tr("拖动调整右侧栏宽度（双击恢复默认）")}
            width={contextWidth}
            min={CONTEXT_MIN}
            max={maxSide(showNav ? navWidth : 0)}
            defaultWidth={CONTEXT_DEFAULT}
            onPreview={setCtxW}
            onCommit={(w) => {
              useSettings.getState().updateLayout({ contextWidth: w });
              setCtxW(null);
            }}
          />
          <ContextPanel />
        </div>
      )}

      {showStatus && (
        <div className={styles.status}>
          <StatusBar capture={capture} />
        </div>
      )}

      <LibraryDialog open={libraryOpen} onClose={() => setLibraryOpen(false)} />
      <TrayHint />
      <Toast />
      <BatchDialog />
      {!immersive && <BatchChip />}
      {immersive && (
        <ImmersiveBar
          revealOnHover={immersivePrefs?.revealOnHover ?? true}
          onWordbook={() => go("wordbook")}
          onExport={() => lookupPage.current?.openExport()}
          onSettings={() => {
            go("settings");
            scrollRoot().scrollTo(0, 0);
          }}
        />
      )}
    </div>
    </div>
    </div>
  );
}
