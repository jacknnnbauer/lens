import { getCurrentWindow } from "@tauri-apps/api/window";
import React from "react";
import ReactDOM from "react-dom/client";
import "./theme/base.css";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { MainWindow } from "./windows/main/MainWindow";
import { PopupWindow } from "./windows/popup/PopupWindow";

// 主窗口与迷你弹窗共用同一个页面入口，按窗口标签决定渲染哪个
let label = "main";
try {
  label = getCurrentWindow().label;
} catch {
  // 不在 Tauri 里（如单元测试、普通浏览器）按主窗口处理
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>{label === "popup" ? <PopupWindow /> : <MainWindow />}</ErrorBoundary>
  </React.StrictMode>,
);
