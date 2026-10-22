// 设置 › 布局 › 右侧栏内容：勾选显示哪些、拖动（或 Alt+↑/↓）调整顺序，还可以把词典加进去。

import { GripIcon } from "../../components/icons";
import { BUILTIN_SECTIONS, DICT_PREFIX, dictIdOf, normalizePanel } from "../../components/panelSections";
import { useDragReorder } from "../../components/useDragReorder";
import { tr } from "../../i18n";
import type { PanelItem } from "../../ipc/settings";
import { useDicts } from "../../stores/dicts";
import { useSettings } from "../../stores/settings";
import styles from "./PanelSettings.module.css";

export function PanelSettings() {
  const panel = normalizePanel(useSettings((s) => s.settings?.layout.panel));
  const updateLayout = useSettings((s) => s.updateLayout);
  const dicts = useDicts((s) => s.dicts).filter((d) => d.kind !== "online");
  const save = (next: PanelItem[]) => updateLayout({ panel: next });
  const ids = panel.map((p) => p.id);
  const drag = useDragReorder(ids, (order) => save(order.map((id) => panel.find((p) => p.id === id) ?? { id, show: true })));

  const nameOf = (id: string) => {
    const d = dictIdOf(id);
    if (d) return dicts.find((x) => x.id === d)?.name ?? tr("（已移除的词典）");
    return tr(BUILTIN_SECTIONS.find((s) => s.id === id)?.name ?? id);
  };
  const move = (id: string, dir: -1 | 1) => {
    const i = ids.indexOf(id);
    const j = i + dir;
    if (i < 0 || j < 0 || j >= panel.length) return;
    const next = [...panel];
    [next[i], next[j]] = [next[j], next[i]];
    save(next);
  };
  const addable = dicts.filter((d) => !ids.includes(DICT_PREFIX + d.id));

  return (
    <fieldset className={styles.box}>
      <legend>{tr("右侧栏内容")}</legend>
      <p className={styles.hint}>{tr("勾选要显示的内容，拖动左侧把手（或 Alt+↑ / Alt+↓）调整先后。加进来的词典只显示当前配置组里查到的词条。")}</p>
      <div className={styles.list}>
        {panel.map((p, i) => {
          const isDict = dictIdOf(p.id) !== null;
          return (
            <div
              key={p.id}
              ref={drag.rowRef(p.id)}
              className={styles.row}
              {...drag.rowState(p.id, i)}
              onKeyDown={(e) => {
                if (!e.altKey || (e.key !== "ArrowUp" && e.key !== "ArrowDown")) return;
                e.preventDefault();
                move(p.id, e.key === "ArrowUp" ? -1 : 1);
              }}
            >
              <button type="button" className={styles.grip} aria-label={tr("拖动调整顺序（Alt+↑ / Alt+↓）") + ` · ${nameOf(p.id)}`} {...drag.handleProps(p.id)}>
                <GripIcon size={14} />
              </button>
              <label>
                <input type="checkbox" checked={p.show} onChange={(e) => save(panel.map((x) => (x.id === p.id ? { ...x, show: e.target.checked } : x)))} />
                {nameOf(p.id)}
                {isDict && <small>{tr("词典")}</small>}
              </label>
              {isDict && (
                <button type="button" className={styles.remove} onClick={() => save(panel.filter((x) => x.id !== p.id))}>
                  {tr("移除")}
                </button>
              )}
            </div>
          );
        })}
      </div>
      {addable.length > 0 && (
        <label className={styles.add}>
          <span>{tr("添加词典")}</span>
          <select
            value=""
            onChange={(e) => {
              if (e.target.value) save([...panel, { id: DICT_PREFIX + e.target.value, show: true }]);
            }}
          >
            <option value="">{tr("选择一本词典…")}</option>
            {addable.map((d) => (
              <option key={d.id} value={d.id}>
                {d.name}
              </option>
            ))}
          </select>
        </label>
      )}
    </fieldset>
  );
}
