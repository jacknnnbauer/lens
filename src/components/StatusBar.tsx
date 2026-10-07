// 底部状态栏（画板 01）：词典状态、当前场景配置组、常用快捷键。

import type { CaptureStatus } from "../ipc/capture";
import { useDicts } from "../stores/dicts";
import { useSettings } from "../stores/settings";
import styles from "./StatusBar.module.css";

export function StatusBar({ capture }: { capture: CaptureStatus | null }) {
  const dicts = useDicts((s) => s.dicts);
  const profiles = useDicts((s) => s.profiles);
  const active = useSettings((s) => s.settings?.activeProfile);
  const loaded = dicts.filter((d) => d.loaded).length;
  const failed = dicts.length - loaded;
  const profile = profiles.find((p) => p.id === active);

  return (
    <footer className={styles.bar}>
      <span className={styles.item}>
        <span className={styles.dot} aria-hidden="true" />
        {loaded} 部离线词典 · 索引就绪
      </span>
      {failed > 0 && <span className={styles.warn}>{failed} 部未能加载</span>}
      {profile && <span>配置组：{profile.name}</span>}
      {capture && (
        <span className={capture.hotkeyRegistered ? undefined : styles.warn}>
          划词：{[capture.hotkeyRegistered && capture.hotkey, capture.doubleCopy && "Ctrl+C+C"].filter(Boolean).join(" / ") || "未启用"}
        </span>
      )}
      <span className={styles.keys}>Ctrl+K 搜索　Tab 切典　Space 展开　Ctrl+1/2/3 配置组　Ctrl+L 词典库</span>
    </footer>
  );
}
