// 设置 › 快捷键：应用内快捷键逐项可改（点击后按新组合键，Esc 取消），改过的可单独恢复默认。

import { useState } from "react";
import { checkAppKey, KEY_ACTIONS, keyFor, showCombo, type KeyAction } from "../../keys";
import { useSettings } from "../../stores/settings";
import type { DeepPartial, Settings } from "../../ipc/settings";
import styles from "./SettingsPage.module.css";
import { tr } from "../../i18n";

function KeyRow({ id, def, label }: { id: KeyAction; def: string; label: string }) {
  const keys = useSettings((s) => s.settings?.keys);
  const globalHotkey = useSettings((s) => s.settings?.capture.hotkey) ?? "";
  const updateNow = useSettings((s) => s.updateNow);
  const [recording, setRecording] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const value = keyFor(id, keys);
  const changed = value !== def;
  // JSON Merge Patch：null 删掉这一项，回到默认
  const set = (v: string | null) => void updateNow({ keys: { [id]: v } } as unknown as DeepPartial<Settings>);

  return (
    <tr>
      <th scope="row">{tr(label)}</th>
      <td>
        <span className={styles.appKey}>
          <button
            type="button"
            className={styles.recorder}
            data-recording={recording || undefined}
            onClick={() => {
              setRecording(true);
              setMessage(null);
            }}
            onBlur={() => setRecording(false)}
            onKeyDown={(e) => {
              if (!recording) return;
              e.preventDefault();
              e.stopPropagation();
              if (e.key === "Escape" && !e.ctrlKey && !e.altKey) {
                setRecording(false);
                return;
              }
              const r = checkAppKey(e, id, keys, globalHotkey, tr);
              if (!r) return;
              if (!r.ok) {
                setMessage(r.reason);
                return;
              }
              setRecording(false);
              setMessage(null);
              if (r.value !== value) set(r.value === def ? null : r.value);
            }}
          >
            {recording ? tr("请按下新的组合键…（Esc 取消）") : <kbd>{showCombo(value)}</kbd>}
          </button>
          {changed && (
            <button type="button" className={styles.ghost} onClick={() => set(null)} title={tr("恢复默认 {key}", { key: showCombo(def) })}>
              {tr("恢复默认")}
            </button>
          )}
        </span>
        {message && (
          <small className={styles.hint} role="status">
            {message}
          </small>
        )}
      </td>
    </tr>
  );
}

export function AppKeys() {
  return (
    <table className={styles.keys}>
      <caption className="sr-only">{tr("可自定义的应用内快捷键")}</caption>
      <tbody>
        {KEY_ACTIONS.map((a) => (
          <KeyRow key={a.id} {...a} />
        ))}
      </tbody>
    </table>
  );
}
