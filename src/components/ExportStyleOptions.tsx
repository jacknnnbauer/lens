// 导出样式 / 纸张的选项：单词导出、批量导出、设置页共用。
// 样式除了纸本白底、跟随当前主题、黑白省墨，每套主题（内置 12 套 + 自定义）都可以直接选。

import { tr } from "../i18n";
import { useSettings } from "../stores/settings";
import { allThemes, type CustomTheme } from "../theme/themes";

const NO_CUSTOM: CustomTheme[] = [];

/** [值, 显示名]；显示名已翻译 */
export function exportStyleChoices(customs: readonly CustomTheme[] = NO_CUSTOM): Array<[string, string]> {
  return [
    ["paperWhite", tr("纸本（白底打印版）")],
    ["followTheme", tr("跟随当前主题")],
    ["inkSaving", tr("黑白省墨")],
    ...allThemes(customs).map((t): [string, string] => [`theme:${t.id}`, tr("主题 · {name}", { name: t.custom ? t.name : tr(t.name) })]),
  ];
}

export const PAPER_CHOICES: Array<[string, string]> = [
  ["a4", "A4 纵向"],
  ["a3", "A3 纵向"],
];

export function ExportStyleOptions() {
  const customs = useSettings((s) => s.settings?.customThemes) ?? NO_CUSTOM;
  const [base, themes] = [exportStyleChoices(customs).slice(0, 3), exportStyleChoices(customs).slice(3)];
  return (
    <>
      {base.map(([v, name]) => (
        <option key={v} value={v}>
          {name}
        </option>
      ))}
      <optgroup label={tr("按主题配色")}>
        {themes.map(([v, name]) => (
          <option key={v} value={v}>
            {name}
          </option>
        ))}
      </optgroup>
    </>
  );
}

export function PaperOptions() {
  return (
    <>
      {PAPER_CHOICES.map(([v, name]) => (
        <option key={v} value={v}>
          {tr(name)}
        </option>
      ))}
    </>
  );
}
