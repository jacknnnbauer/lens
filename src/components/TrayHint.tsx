// 第一次在「隐藏到托盘」模式下点关闭：说明 Lens 会留在后台，确认后才隐藏（以后直接隐藏）。

import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { useEffect, useRef } from "react";
import { tr } from "../i18n";
import styles from "./TrayHint.module.css";

export function TrayHint() {
  const dialog = useRef<HTMLDialogElement>(null);
  const ok = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    const off = getCurrentWebviewWindow().listen("app:tray-hint", () => {
      const d = dialog.current;
      if (d && !d.open) {
        d.showModal();
        // 默认按钮是「隐藏到托盘」：Enter 不会误退出
        ok.current?.focus();
      }
    });
    return () => void off.then((f) => f());
  }, []);

  const hide = () => {
    dialog.current?.close();
    void invoke("hide_to_tray");
  };

  return (
    <dialog ref={dialog} className={styles.dialog} aria-labelledby="tray-hint-title">
      <h2 id="tray-hint-title" className={styles.title}>
        {tr("Lens 会留在后台")}
      </h2>
      <p className={styles.text}>
        {tr("窗口隐藏后，在任何程序里划词按快捷键或 Ctrl+C+C 仍然可以查词。点任务栏右下角的托盘图标重新打开，右键托盘图标选「退出」才会真正关闭。")}
      </p>
      <p className={styles.hint}>{tr("以后点关闭直接隐藏，不再提示。可在设置 › 常规里改成「退出 Lens」。")}</p>
      <div className={styles.actions}>
        <button type="button" className={styles.ghost} onClick={() => void invoke("quit_app")}>
          {tr("退出 Lens")}
        </button>
        <button ref={ok} type="button" className={styles.primary} onClick={hide}>
          {tr("知道了，隐藏到托盘")}
        </button>
      </div>
    </dialog>
  );
}
