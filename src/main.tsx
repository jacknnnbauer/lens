import { getCurrentWindow } from "@tauri-apps/api/window";
import React, { lazy, Suspense } from "react";
import ReactDOM from "react-dom/client";
import "./theme/base.css";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { LangRoot } from "./i18n";
import { MainWindow } from "./windows/main/MainWindow";

// 主窗口在启动的关键路径上，直接打包进入口；迷你弹窗、导出窗口用到时再加载自己的代码
const PopupWindow = lazy(() => import("./windows/popup/PopupWindow").then((m) => ({ default: m.PopupWindow })));
const ExportRenderWindow = lazy(() => import("./windows/export/ExportRenderWindow").then((m) => ({ default: m.ExportRenderWindow })));

// 开发模式下暴露状态仓库，方便通过 WebView2 调试端口检查（正式构建不包含）
if (import.meta.env.DEV) {
  void Promise.all([
    import("./stores/lookup"),
    import("./stores/settings"),
    import("./stores/batch"),
    import("./export/run"),
    import("./export-templates/template"),
    import("./export/resources"),
    import("./export-templates/article"),
    import("./dict-render/prepare"),
  ]).then(([l, s, b, r, t, res, art, prep]) => {
    Object.assign(window, {
      __lens: {
        useLookup: l.useLookup,
        useSettings: s.useSettings,
        useBatch: b.useBatch,
        runExport: r.runExport,
        buildExportBody: t.buildExportBody,
        inlineEntry: res.inlineEntry,
        buildArticleBody: art.buildArticleBody,
        prepareEntry: prep.prepareEntry,
      },
    });
  });
}

// 主窗口、迷你弹窗、隐藏的导出窗口共用同一个页面入口，按窗口标签决定渲染哪个
let label = "main";
try {
  label = getCurrentWindow().label;
} catch {
  // 不在 Tauri 里（如单元测试、普通浏览器）按主窗口处理
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <Suspense fallback={null}>
        {label === "export" || label.startsWith("export-") ? (
          // 导出窗口的语言跟随每个导出任务
          <ExportRenderWindow />
        ) : (
          <LangRoot>{label === "popup" ? <PopupWindow /> : <MainWindow />}</LangRoot>
        )}
      </Suspense>
    </ErrorBoundary>
  </React.StrictMode>,
);
