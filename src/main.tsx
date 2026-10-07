import React from "react";
import ReactDOM from "react-dom/client";
import "./theme/base.css";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { MainWindow } from "./windows/main/MainWindow";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <MainWindow />
    </ErrorBoundary>
  </React.StrictMode>,
);
