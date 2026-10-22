// 主题库（SPEC 9.5、画板 09）：12 套内置主题 + 自定义主题；大预览、复制并编辑、导入导出 .lens-theme。
// 颜色格式由 Rust 校验；对比度在这里检查（编辑时实时提示，导入后报告）。

import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { useEffect, useState, type CSSProperties } from "react";
import { themeScope, ThemeThumb } from "../../components/ThemeThumb";
import { tr } from "../../i18n";
import { deleteCustomTheme, exportTheme, importTheme, saveCustomTheme } from "../../ipc/settings";
import { useSettings } from "../../stores/settings";
import {
  allThemes,
  contrastIssues,
  customVars,
  isBuiltinTheme,
  readBuiltinColors,
  THEME_VARS,
  type ContrastIssue,
  type CustomTheme,
  type ThemeInfo,
  type ThemeVar,
} from "../../theme/themes";
import { useAppliedTheme } from "../../theme/useApplyTheme";
import { scrollRoot } from "../../windows/main/scroll";
import styles from "./ThemeLibraryPage.module.css";

const VAR_LABELS: Record<ThemeVar, string> = {
  bg: "背景",
  panel: "卡片",
  raised: "悬停 / 凸起",
  line: "分隔线",
  text: "正文",
  sub: "次要文字",
  faint: "提示文字",
  accent: "强调色",
  "accent-ink": "强调色上的文字",
  warn: "警示色",
};

const NO_CUSTOM: CustomTheme[] = [];

/** 预览：一张缩小的查词页（搜索框 + 词典卡片） */
function ThemePreview({ theme, accent, draft }: { theme: ThemeInfo; accent: string; draft?: CustomTheme }) {
  const scope = draft
    ? { "data-theme": draft.dark ? "dark" : "paper", "data-accent": accent, style: customVars(draft) as CSSProperties }
    : themeScope(theme, accent);
  return (
    <div className={styles.preview} {...scope} aria-label={tr("主题预览")} role="img">
      <div className={styles.pSearch}>
        <span>ubiquitous</span>
        <i className={styles.pBtn}>{tr("查询")}</i>
      </div>
      <div className={styles.pCard}>
        <div className={styles.pHead}>
          <b>{tr("牛津高阶")}</b>
          <small>{tr("在线")}</small>
        </div>
        <p className={styles.pWord}>
          ubiquitous <span>/juːˈbɪkwɪtəs/</span>
        </p>
        <p className={styles.pSense}>
          <em>adj.</em> {tr("无所不在的；十分普遍的")}
        </p>
        <p className={styles.pEx}>Smartphones are now ubiquitous.</p>
        <p className={styles.pFaint}>{tr("来源：牛津高阶英汉双解词典")}</p>
        <p className={styles.pWarn}>{tr("注意：常与 become 连用")}</p>
      </div>
    </div>
  );
}

function ContrastList({ issues }: { issues: ContrastIssue[] }) {
  if (issues.length === 0) return <p className={styles.ok}>{tr("对比度检查通过")}</p>;
  return (
    <ul className={styles.issues} aria-label={tr("对比度不足")}>
      {issues.map((i) => (
        <li key={i.label}>
          {tr(i.label)} <code>{i.ratio.toFixed(2)} : 1</code> {tr("（建议 ≥ {min} : 1）", { min: i.min })}
        </li>
      ))}
    </ul>
  );
}

function Editor({ draft, onChange, onSave, onCancel, busy }: { draft: CustomTheme; onChange: (d: CustomTheme) => void; onSave: () => void; onCancel: () => void; busy: boolean }) {
  const setColor = (v: ThemeVar, c: string) => onChange({ ...draft, colors: { ...draft.colors, [v]: c } });
  return (
    <form
      className={styles.editor}
      onSubmit={(e) => {
        e.preventDefault();
        onSave();
      }}
    >
      <label className={styles.field}>
        <span>{tr("名称")}</span>
        <input value={draft.name} maxLength={40} onChange={(e) => onChange({ ...draft, name: e.target.value })} required />
      </label>
      <label className={styles.check}>
        <input type="checkbox" checked={draft.dark} onChange={(e) => onChange({ ...draft, dark: e.target.checked })} />
        {tr("暗色主题（跟随系统时归入暗色一组）")}
      </label>
      <div className={styles.colors}>
        {THEME_VARS.map((v) => (
          <label key={v} className={styles.color}>
            <input type="color" value={draft.colors[v]} onChange={(e) => setColor(v, e.target.value.toUpperCase())} aria-label={tr(VAR_LABELS[v])} />
            <span>{tr(VAR_LABELS[v])}</span>
            <input
              className={styles.hex}
              value={draft.colors[v]}
              spellCheck={false}
              pattern="#[0-9A-Fa-f]{6}"
              aria-label={tr("{name} 的色值", { name: tr(VAR_LABELS[v]) })}
              onChange={(e) => setColor(v, e.target.value.trim())}
            />
          </label>
        ))}
      </div>
      <ContrastList issues={contrastIssues(draft.colors)} />
      <div className={styles.actions}>
        <button type="submit" className={styles.primary} disabled={busy}>
          {tr("保存")}
        </button>
        <button type="button" onClick={onCancel}>
          {tr("取消")}
        </button>
      </div>
    </form>
  );
}

export function ThemeLibraryPage({ onBack }: { onBack: () => void }) {
  const settings = useSettings((s) => s.settings);
  const update = useSettings((s) => s.update);
  const appliedTheme = useAppliedTheme((s) => s.theme) ?? "paper";
  const customs = settings?.customThemes ?? NO_CUSTOM;
  const themes = allThemes(customs);
  const [selected, setSelected] = useState<string>(appliedTheme);
  const [draft, setDraft] = useState<CustomTheme | null>(null);
  const [message, setMessage] = useState<{ text: string; issues?: ContrastIssue[]; error?: boolean } | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    scrollRoot().scrollTo(0, 0);
  }, []);
  useEffect(() => setConfirmDelete(false), [selected]);

  if (!settings) return null;
  const a = settings.appearance;
  const accent = a.accent;
  const current = themes.find((t) => t.id === selected) ?? themes[0];

  // Rust 改了 settings.json：先把前端未保存的改动写回，再重新读
  const reload = async () => {
    await useSettings.getState().updateNow({});
    await useSettings.getState().load();
  };

  const startCopy = (t: ThemeInfo) => {
    const colors = t.custom ? { ...t.custom.colors } : isBuiltinTheme(t.id) ? readBuiltinColors(t.id) : readBuiltinColors("paper");
    setDraft({ id: "", name: tr("{name} 副本", { name: tr(t.name) }), dark: t.dark, colors });
    setMessage(null);
  };

  const saveDraft = async () => {
    if (!draft) return;
    setBusy(true);
    try {
      const saved = await saveCustomTheme(draft);
      await reload();
      setDraft(null);
      setSelected(saved.id);
      setMessage({ text: tr("已保存「{name}」", { name: saved.name }) });
    } catch (e) {
      setMessage({ text: String(e), error: true });
    } finally {
      setBusy(false);
    }
  };

  const doImport = async () => {
    const path = await openDialog({ multiple: false, filters: [{ name: tr("Lens 主题"), extensions: ["lens-theme"] }] });
    if (typeof path !== "string") return;
    try {
      const t = await importTheme(path);
      await reload();
      setSelected(t.id);
      const issues = contrastIssues(t.colors);
      setMessage({
        text: issues.length ? tr("已导入「{name}」，但有 {n} 处对比度不足：", { name: t.name, n: issues.length }) : tr("已导入「{name}」，对比度检查通过", { name: t.name }),
        issues: issues.length ? issues : undefined,
      });
    } catch (e) {
      setMessage({ text: tr("导入失败：{error}", { error: String(e) }), error: true });
    }
  };

  const doExport = async (t: CustomTheme) => {
    const path = await saveDialog({ defaultPath: `${t.name}.lens-theme`, filters: [{ name: tr("Lens 主题"), extensions: ["lens-theme"] }] });
    if (!path) return;
    try {
      await exportTheme(path, t);
      setMessage({ text: tr("已导出到 {path}", { path }) });
    } catch (e) {
      setMessage({ text: tr("导出失败：{error}", { error: String(e) }), error: true });
    }
  };

  const doDelete = async (t: CustomTheme) => {
    try {
      useSettings.setState({ settings: await deleteCustomTheme(t.id) });
      setSelected(a.followSystem ? appliedTheme : "paper");
      setMessage({ text: tr("已删除「{name}」", { name: t.name }) });
    } catch (e) {
      setMessage({ text: String(e), error: true });
    }
  };

  const inUse = !a.followSystem && a.theme === current.id;
  const builtinCount = themes.filter((t) => !t.custom).length;

  return (
    <div className={styles.page}>
      <button type="button" className={styles.back} onClick={onBack}>
        {tr("← 返回设置")}
      </button>
      <header className={styles.header}>
        <h1>{tr("主题库")}</h1>
        <p className={styles.sub}>{tr("{a} 套内置 · {b} 套自定义", { a: builtinCount, b: customs.length })}</p>
        <button type="button" className={styles.importBtn} onClick={() => void doImport()}>
          {tr("导入主题…")}
        </button>
      </header>

      {message && (
        <div className={styles.message} role={message.error ? "alert" : "status"} data-error={message.error || undefined}>
          {message.text}
          {message.issues && <ContrastList issues={message.issues} />}
        </div>
      )}

      <div className={styles.layout}>
        <div className={styles.grid} role="radiogroup" aria-label={tr("主题")}>
          {themes.map((t) => (
            <button
              key={t.id}
              type="button"
              role="radio"
              aria-checked={t.id === current.id}
              className={styles.tile}
              data-selected={t.id === current.id || undefined}
              onClick={() => {
                setSelected(t.id);
                setDraft(null);
              }}
            >
              <ThemeThumb theme={t} accent={accent} />
              <span className={styles.tileLabel}>
                {t.custom ? t.name : tr(t.name)}
                <small>{t.dark ? tr("暗色") : tr("亮色")}</small>
                {t.id === appliedTheme && <small className={styles.badge}>{tr("当前")}</small>}
              </span>
            </button>
          ))}
        </div>

        <aside className={styles.detail} data-editing={draft ? true : undefined} aria-label={tr("主题详情")}>
          <ThemePreview theme={current} accent={accent} draft={draft ?? undefined} />
          {draft ? (
            <Editor draft={draft} onChange={setDraft} onSave={() => void saveDraft()} onCancel={() => setDraft(null)} busy={busy} />
          ) : (
            <>
              <h2>{current.custom ? current.name : tr(current.name)}</h2>
              {current.note && <p className={styles.note}>{tr(current.note)}</p>}
              <p className={styles.hint}>
                {current.accentable ? tr("可在设置里搭配五种强调色。") : current.custom ? tr("自定义主题：强调色由主题自己决定。") : tr("自带强调色，不受强调色设置影响。")}
              </p>
              <div className={styles.actions}>
                <button type="button" className={styles.primary} disabled={inUse} onClick={() => update({ appearance: { theme: current.id, followSystem: false } })}>
                  {inUse ? tr("正在使用") : tr("使用此主题")}
                </button>
                <button type="button" onClick={() => startCopy(current)}>
                  {tr("复制并编辑")}
                </button>
                {current.custom && (
                  <>
                    <button type="button" onClick={() => current.custom && setDraft({ ...current.custom, colors: { ...current.custom.colors } })}>
                      {tr("编辑")}
                    </button>
                    <button type="button" onClick={() => current.custom && void doExport(current.custom)}>
                      {tr("导出…")}
                    </button>
                    {confirmDelete ? (
                      <button type="button" className={styles.danger} onClick={() => current.custom && void doDelete(current.custom)}>
                        {tr("确认删除")}
                      </button>
                    ) : (
                      <button type="button" onClick={() => setConfirmDelete(true)}>
                        {tr("删除")}
                      </button>
                    )}
                  </>
                )}
              </div>
              {current.custom && <ContrastList issues={contrastIssues(current.custom.colors)} />}
            </>
          )}
        </aside>
      </div>
    </div>
  );
}
