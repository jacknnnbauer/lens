// 全局快捷键注册失败（常见：被其他程序占用）时的提示条，可直接换一个快捷键。
// 完整的快捷键设置在 M5 设置页。

import { useEffect, useState } from "react";
import { captureStatus, onCaptureStatus, type CaptureStatus } from "../ipc/capture";
import { useSettings } from "../stores/settings";
import styles from "./HotkeyNotice.module.css";

export function useCaptureStatus(): CaptureStatus | null {
  const [status, setStatus] = useState<CaptureStatus | null>(null);
  useEffect(() => {
    void captureStatus().then(setStatus, () => setStatus(null));
    const off = onCaptureStatus(setStatus);
    return () => void off.then((f) => f());
  }, []);
  return status;
}

export function HotkeyNotice({ status }: { status: CaptureStatus | null }) {
  const updateNow = useSettings((s) => s.updateNow);
  const [value, setValue] = useState("Ctrl+Alt+L");
  const [dismissed, setDismissed] = useState(false);
  if (!status || status.hotkeyRegistered || dismissed) return null;

  return (
    <div role="status" className={styles.bar}>
      <span>
        全局快捷键 <kbd>{status.hotkey}</kbd> {status.hotkeyError ?? "注册失败"}。
        {status.doubleCopy && "划词仍可用 Ctrl+C+C。"}
      </span>
      <form
        className={styles.form}
        onSubmit={(e) => {
          e.preventDefault();
          if (value.trim()) void updateNow({ capture: { hotkey: value.trim() } });
        }}
      >
        <label className="sr-only" htmlFor="hotkey-input">
          新的快捷键
        </label>
        <input id="hotkey-input" className={styles.input} value={value} onChange={(e) => setValue(e.target.value)} spellCheck={false} />
        <button type="submit" className={styles.btn}>
          改用
        </button>
      </form>
      <button type="button" className={styles.close} aria-label="关闭提示" onClick={() => setDismissed(true)}>
        ×
      </button>
    </div>
  );
}
