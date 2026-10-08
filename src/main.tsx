import { getCurrentWindow } from "@tauri-apps/api/window";
import React from "react";
import ReactDOM from "react-dom/client";
import "./theme/base.css";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { MainWindow } from "./windows/main/MainWindow";
import { ExportRenderWindow } from "./windows/export/ExportRenderWindow";
import { PopupWindow } from "./windows/popup/PopupWindow";
import { runExport } from "./export/run";
import { inlineEntry } from "./export/resources";
import { buildExportBody } from "./export-templates/template";
import { useBatch } from "./stores/batch";
import { useLookup } from "./stores/lookup";
import { useSettings } from "./stores/settings";

// 开发模式下暴露状态仓库，方便通过 WebView2 调试端口检查（正式构建会被摇树去掉）
if (import.meta.env.DEV) {
  Object.assign(window, { __lens: { useLookup, useSettings, useBatch, runExport, buildExportBody, inlineEntry } });
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
      {label === "popup" ? <PopupWindow /> : label === "export" ? <ExportRenderWindow /> : <MainWindow />}
    </ErrorBoundary>
  </React.StrictMode>,
);
