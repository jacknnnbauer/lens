// 设置 › 场景配置组：每个场景各自勾选用哪些词典（可以完全不同，也可以有重叠）。
// 词典的先后顺序是共用的，在词典库（Ctrl+L）里拖动调整。

import { useDicts } from "../../stores/dicts";
import { useSettings } from "../../stores/settings";
import { tr } from "../../i18n";
import styles from "./SettingsPage.module.css";

export function ProfileSettings() {
  const dicts = useDicts((s) => s.dicts);
  const profiles = useDicts((s) => s.profiles);
  const setInProfile = useDicts((s) => s.setInProfile);
  const active = useSettings((s) => s.settings?.activeProfile);
  const update = useSettings((s) => s.update);
  // 设置 › 取词里选了「自选词典」：多一列，勾查词小窗用的词典
  const popupCustom = useSettings((s) => s.settings?.capture?.popupProfile) === "custom";
  const popupDicts = useSettings((s) => s.settings?.capture?.popupDicts) ?? [];
  const setInPopup = (id: string, on: boolean) =>
    update({ capture: { popupDicts: on ? [...popupDicts.filter((d) => d !== id), id] : popupDicts.filter((d) => d !== id) } });

  return (
    <section id="set-profiles" className={styles.section} aria-labelledby="h-profiles">
      <h2 id="h-profiles">{tr("场景配置组")}</h2>
      {dicts.length === 0 ? (
        <p className={styles.hint}>{tr("还没有导入词典。")}</p>
      ) : (
        <div className={styles.matrixWrap}>
          <table className={styles.matrix}>
            <thead>
              <tr>
                <th scope="col">{tr("词典")}</th>
                {profiles.map((p, i) => (
                  <th key={p.id} scope="col" data-active={p.id === active || undefined}>
                    {tr(p.name)}
                    <small>
                      {tr("{n} 典", { n: p.dictCount })}
                      {i < 9 && ` · Ctrl+${i + 1}`}
                    </small>
                  </th>
                ))}
                {popupCustom && (
                  <th scope="col">
                    {tr("查词小窗")}
                    <small>{tr("{n} 典", { n: dicts.filter((d) => popupDicts.includes(d.id)).length })}</small>
                  </th>
                )}
              </tr>
            </thead>
            <tbody>
              {dicts.map((d) => (
                <tr key={d.id}>
                  <th scope="row">{d.name}</th>
                  {profiles.map((p) => (
                    <td key={p.id} data-active={p.id === active || undefined}>
                      <input
                        type="checkbox"
                        checked={d.profiles.includes(p.id)}
                        aria-label={tr("{profile}用 {dict}", { profile: tr(p.name), dict: d.name })}
                        onChange={(e) => void setInProfile(p.id, d.id, e.target.checked, p.id === active)}
                      />
                    </td>
                  ))}
                  {popupCustom && (
                    <td>
                      <input
                        type="checkbox"
                        checked={popupDicts.includes(d.id)}
                        aria-label={tr("查词小窗用 {dict}", { dict: d.name })}
                        onChange={(e) => setInPopup(d.id, e.target.checked)}
                      />
                    </td>
                  )}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <p className={styles.hint}>
        {tr("勾选每个场景查词时用哪些词典，同一部词典可以用在多个场景。左侧栏或 Ctrl+1 / 2 / 3 切换场景；词典的先后顺序在词典库（Ctrl+L）里拖动调整。")}
      </p>
    </section>
  );
}
