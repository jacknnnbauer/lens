import { Component, type ErrorInfo, type ReactNode } from "react";
import { tr } from "../i18n";

interface State {
  error: Error | null;
}

/** 顶层错误边界（SPEC 15）：渲染出错时显示提示而不是白屏。 */
export class ErrorBoundary extends Component<{ children: ReactNode }, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error(tr("界面渲染出错"), error, info.componentStack);
  }

  render() {
    if (this.state.error) {
      return (
        <div role="alert" style={{ padding: 24 }}>
          <h1 style={{ fontSize: 16 }}>{tr("界面出错了")}</h1>
          <pre style={{ whiteSpace: "pre-wrap", color: "var(--warn)" }}>{this.state.error.message}</pre>
          <button type="button" onClick={() => this.setState({ error: null })}>
            {tr("重试")}
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}
