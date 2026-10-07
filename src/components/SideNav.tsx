// 左侧导航（画板 01）：页面入口、场景配置组（Ctrl+1/2/3）、本次会话最近查询。
// 生词本、设置页随 M4/M5 加入。

import { useDicts } from "../stores/dicts";
import { useLookup } from "../stores/lookup";
import { useRoute } from "../stores/route";
import { useSettings } from "../stores/settings";
import styles from "./SideNav.module.css";

interface Props {
  onFocusSearch: () => void;
  onOpenHistory: () => void;
  onOpenLibrary: () => void;
  onSwitchProfile: (id: string) => void;
  onLookup: (word: string) => void;
}

export function SideNav({ onFocusSearch, onOpenHistory, onOpenLibrary, onSwitchProfile, onLookup }: Props) {
  const page = useRoute((s) => s.page);
  const profiles = useDicts((s) => s.profiles);
  const active = useSettings((s) => s.settings?.activeProfile);
  const recent = useLookup((s) => s.recent);

  return (
    <nav aria-label="主导航" className={styles.nav}>
      <div className={styles.group}>
        <button type="button" className={styles.item} aria-current={page === "lookup" ? "page" : undefined} onClick={onFocusSearch}>
          查词<kbd>Ctrl+K</kbd>
        </button>
        <button type="button" className={styles.item} aria-current={page === "history" ? "page" : undefined} onClick={onOpenHistory}>
          历史<kbd>Ctrl+H</kbd>
        </button>
        <button type="button" className={styles.item} onClick={onOpenLibrary}>
          词典库<kbd>Ctrl+L</kbd>
        </button>
      </div>

      <div className={styles.group}>
        <div className={styles.label} id="nav-profiles">
          场景配置组
        </div>
        <div role="group" aria-labelledby="nav-profiles" className={styles.group}>
          {profiles.map((p, i) => (
            <button
              key={p.id}
              type="button"
              className={styles.item}
              aria-pressed={p.id === active}
              onClick={() => onSwitchProfile(p.id)}
            >
              <span>
                {p.name} · {p.dictCount} 典
              </span>
              {i < 9 && <kbd>Ctrl+{i + 1}</kbd>}
            </button>
          ))}
        </div>
      </div>

      <div className={styles.group}>
        <div className={styles.label}>最近</div>
        {recent.length === 0 ? (
          <p className={styles.empty}>本次还没有查询</p>
        ) : (
          recent.map((w) => (
            <button key={w} type="button" className={styles.word} onClick={() => onLookup(w)}>
              {w}
            </button>
          ))
        )}
      </div>
    </nav>
  );
}
