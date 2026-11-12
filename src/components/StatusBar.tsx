// 底部状态栏（画板 01）：词典状态、当前场景配置组、常用快捷键。

import type { CaptureStatus } from "../ipc/capture";
import { useDicts } from "../stores/dicts";
import { useSettings } from "../stores/settings";
import styles from "./StatusBar.module.css";
import { tr } from "../i18n";
import { useKeyLabel } from "../keys";

export function StatusBar({ capture }: { capture: CaptureStatus | null }) {
  const dicts = useDicts((s) => s.dicts);
  const profiles = useDicts((s) => s.profiles);
  const active = useSettings((s) => s.settings?.activeProfile);
  const loaded = dicts.filter((d) => d.loaded).length;
  const failed = dicts.length - loaded;
  const profile = profiles.find((p) => p.id === active);
  const kSearch = useKeyLabel("focusSearch");
  const kSave = useKeyLabel("save");
  const kBook = useKeyLabel("wordbook");
  const kHist = useKeyLabel("history");

  return (
    <footer className={styles.bar} data-ui="statusbar">
      <span className={styles.item}>
        <span className={styles.dot} aria-hidden="true" />
        {tr("{n} 部离线词典 · 索引就绪", { n: loaded })}
      </span>
      {failed > 0 && <span className={styles.warn}>{tr("{n} 部未能加载", { n: failed })}</span>}
      {profile && <span>{tr("配置组：{name}", { name: tr(profile.name) })}</span>}
      {capture && (
        <span className={capture.hotkeyRegistered ? undefined : styles.warn}>
          {tr("划词：{how}", { how: [capture.hotkeyRegistered && capture.hotkey, capture.doubleCopy && "Ctrl+C+C"].filter(Boolean).join(" / ") || tr("未启用") })}
        </span>
      )}
      <span className={styles.keys}>
        {tr("{k} 搜索 {s} 收藏 {b} 生词本 {h} 历史 Ctrl+1/2/3 配置组", { k: kSearch, s: kSave, b: kBook, h: kHist })}
      </span>
    </footer>
  );
}
