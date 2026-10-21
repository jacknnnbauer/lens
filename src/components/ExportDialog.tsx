// 导出词条对话框（SPEC 11、画板 07）：PDF / HTML，卡片式 / 文档式，三种样式，A4 / Letter。

import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { inlineEntry } from "../export/resources";
import { runExport } from "../export/run";
import {
  buildExportBody,
  exportTheme,
  isoDate,
  safeFileName,
  type ExportLayout,
  type ExportOptions,
  type ExportStyle,
  type Paper,
} from "../export-templates/template";
import type { ContextRow } from "../ipc/history";
import type { Card } from "../stores/lookup";
import { useSettings } from "../stores/settings";
import { useAppliedTheme } from "../theme/useApplyTheme";
import styles from "./ExportDialog.module.css";
import { ExportStyleOptions, PaperOptions } from "./ExportStyleOptions";
import { CloseIcon, GripIcon } from "./icons";
import { useDragReorder } from "./useDragReorder";
import { getLang, tr } from "../i18n";

interface Props {
  open: boolean;
  onClose: () => void;
  word: string;
  /** 当前查询命中的词典卡片，按查词顺序 */
  cards: Card[];
}

type Phase = { kind: "idle" } | { kind: "busy" } | { kind: "done"; path: string } | { kind: "error"; message: string };

export function ExportDialog({ open, onClose, word, cards }: Props) {
  const dialog = useRef<HTMLDialogElement>(null);
  const prefs = useSettings((s) => s.settings?.export);
  const accent = useSettings((s) => s.settings?.appearance.accent) ?? "blue";
  const update = useSettings((s) => s.update);
  const appliedTheme = useAppliedTheme((s) => s.theme) ?? "paper";

  const [format, setFormat] = useState<"pdf" | "html">("pdf");
  const [layout, setLayout] = useState<ExportLayout>("cards");
  const [style, setStyle] = useState<ExportStyle>("paperWhite");
  const [paper, setPaper] = useState<Paper>("a4");
  const [options, setOptions] = useState<ExportOptions>({ phonetics: true, examples: true, context: true, sourceDate: true, dictNames: true });
  const [selected, setSelected] = useState<Set<string>>(new Set());
  /** 本次导出里词典的先后（默认按词典库顺序，可在这里调整） */
  const [order, setOrder] = useState<string[]>([]);
  const [context, setContext] = useState<ContextRow | null>(null);
  const [name, setName] = useState("");
  const [phase, setPhase] = useState<Phase>({ kind: "idle" });

  const hits = useMemo(() => cards.filter((c) => c.status === "hit"), [cards]);

  // 每次打开：按设置恢复版式/样式/纸张，默认勾选全部命中的词典，取这个词最近的语境句
  useEffect(() => {
    const d = dialog.current;
    if (!d) return;
    if (open && !d.open) {
      if (prefs) {
        setLayout(prefs.layout);
        setStyle(prefs.style);
        setPaper(prefs.paper);
      }
      setSelected(new Set(hits.map((c) => c.id)));
      setOrder(hits.map((c) => c.id));
      setName(`${safeFileName(word)}_${isoDate(new Date())}`);
      setPhase({ kind: "idle" });
      setContext(null);
      invoke<ContextRow | null>("latest_context", { query: word }).then(setContext, () => setContext(null));
      d.showModal();
    }
    if (!open && d.open) d.close();
    // 只在打开那一刻初始化
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  const ordered = [...hits].sort((a, b) => {
    const ia = order.indexOf(a.id);
    const ib = order.indexOf(b.id);
    return (ia < 0 ? Infinity : ia) - (ib < 0 ? Infinity : ib);
  });
  const chosen = ordered.filter((c) => selected.has(c.id));
  const move = (id: string, dir: -1 | 1) => {
    const ids = ordered.map((c) => c.id);
    const i = ids.indexOf(id);
    const j = i + dir;
    if (i < 0 || j < 0 || j >= ids.length) return;
    [ids[i], ids[j]] = [ids[j], ids[i]];
    setOrder(ids);
  };
  const drag = useDragReorder(
    ordered.map((c) => c.id),
    setOrder,
  );
  const ext = format === "pdf" ? "pdf" : "html";
  // 词典可以一本都不选，只导出语境句
  const withContext = context != null && options.context;
  const canExport = chosen.length > 0 || withContext;

  const doExport = async () => {
    if (!canExport || phase.kind === "busy") return;
    const fileName = `${safeFileName(name.replace(/\.(pdf|html?)$/i, "")) || "lens"}.${ext}`;
    const path = await saveDialog({
      title: tr("导出词条"),
      defaultPath: prefs?.dir ? `${prefs.dir}\\${fileName}` : fileName,
      filters: [format === "pdf" ? { name: "PDF", extensions: ["pdf"] } : { name: "HTML", extensions: ["html"] }],
    });
    if (!path) return;
    setPhase({ kind: "busy" });
    try {
      const exportCards = await Promise.all(
        chosen.map(async (c) => ({
          id: c.id,
          name: c.name,
          online: c.source !== "local",
          // HTML 要单文件可用：把词典 CSS 与图片内联；PDF 由导出窗口直接加载 lens:// 资源
          entries: format === "html" ? await Promise.all(c.entries.map((e) => inlineEntry(e.html))) : c.entries.map((e) => e.html),
        })),
      );
      const body = buildExportBody({ word, date: new Date(), layout, style, options, cards: exportCards, context });
      const theme = exportTheme(style, appliedTheme);
      await runExport({ jobId: crypto.randomUUID(), format, body, title: `${word} · ${tr("Lens 词镜")}`, theme, accent, path, paper, lang: getLang() });
      update({ export: { layout, style, paper, dir: path.replace(/[\\/][^\\/]*$/, "") } });
      setPhase({ kind: "done", path });
    } catch (e) {
      setPhase({ kind: "error", message: String(e instanceof Error ? e.message : e) });
    }
  };

  const toggle = (id: string) =>
    setSelected((s) => {
      const n = new Set(s);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });

  return (
    <dialog
      ref={dialog}
      className={styles.dialog}
      onClose={onClose}
      aria-labelledby="export-title"
      onKeyDown={(e) => {
        if (e.ctrlKey && e.key === "Enter") {
          e.preventDefault();
          void doExport();
        }
      }}
    >
      <header className={styles.head}>
        <h2 id="export-title">{tr("导出词条")}</h2>
        <span className={styles.word}>{word}</span>
        <button type="button" className={styles.iconBtn} aria-label={tr("关闭")} onClick={onClose}>
          <CloseIcon />
        </button>
      </header>

      <div className={styles.content}>
        <div className={styles.form}>
          <fieldset>
            <legend>{tr("格式")}</legend>
            <div className={styles.formats}>
              {(["pdf", "html"] as const).map((f) => (
                <label key={f} className={styles.format} data-checked={format === f || undefined}>
                  <input type="radio" name="export-format" checked={format === f} onChange={() => setFormat(f)} />
                  <span>
                    <strong>{f.toUpperCase()}</strong>
                    <small>{tr(f === "pdf" ? "打印、归档、发给别人" : "单个文件，浏览器直接打开")}</small>
                  </span>
                </label>
              ))}
            </div>
          </fieldset>

          <fieldset>
            <legend>
              {tr("包含的词典")} <small>{tr("{a} / {b} · 拖动调整先后", { a: chosen.length, b: hits.length })}</small>
            </legend>
            <div className={styles.dicts}>
              {ordered.map((c, i) => (
                <div
                  key={c.id}
                  ref={drag.rowRef(c.id)}
                  className={styles.dictRow}
                  {...drag.rowState(c.id, i)}
                  onKeyDown={(e) => {
                    if (!e.altKey || (e.key !== "ArrowUp" && e.key !== "ArrowDown")) return;
                    e.preventDefault();
                    move(c.id, e.key === "ArrowUp" ? -1 : 1);
                  }}
                >
                  <button
                    type="button"
                    className={styles.grip}
                    aria-label={tr("拖动调整顺序（Alt+↑ / Alt+↓）") + ` · ${c.name}`}
                    title={tr("拖动调整顺序（Alt+↑ / Alt+↓）")}
                    {...drag.handleProps(c.id)}
                  >
                    <GripIcon size={14} />
                  </button>
                  <label>
                    <input type="checkbox" checked={selected.has(c.id)} onChange={() => toggle(c.id)} />
                    {c.name}
                    {c.source !== "local" && <small>{tr("在线")}</small>}
                  </label>
                </div>
              ))}
            </div>
          </fieldset>

          <div className={styles.selects}>
            <label>
              <span>{tr("版式")}</span>
              <select value={layout} onChange={(e) => setLayout(e.target.value as ExportLayout)}>
                <option value="cards">{tr("卡片式（同简洁单栏）")}</option>
                <option value="document">{tr("文档式")}</option>
              </select>
            </label>
            <label>
              <span>{tr("样式")}</span>
              <select value={style} onChange={(e) => setStyle(e.target.value as ExportStyle)}>
                <ExportStyleOptions />
              </select>
            </label>
            <label>
              <span>{tr("纸张")}</span>
              <select value={paper === "letter" ? "a4" : paper} disabled={format === "html"} onChange={(e) => setPaper(e.target.value as Paper)}>
                <PaperOptions />
              </select>
            </label>
          </div>

          <div className={styles.checks}>
            {(
              [
                ["phonetics", "音标"],
                ["examples", "例句"],
                ["context", context ? "我的语境" : "我的语境（这个词还没有）"],
                ["sourceDate", "来源与日期"],
                ["dictNames", "词典名称"],
              ] as const
            ).map(([k, label]) => (
              <label key={k}>
                <input
                  type="checkbox"
                  checked={options[k] !== false}
                  disabled={k === "context" && !context}
                  onChange={(e) => setOptions((o) => ({ ...o, [k]: e.target.checked }))}
                />
                {tr(label)}
              </label>
            ))}
          </div>
        </div>

        <aside className={styles.preview} aria-label={tr("版式示意")}>
          <div className={styles.previewHead}>{tr("示意")}</div>
          <div className={styles.page} data-layout={layout} data-style={style}>
            {layout === "cards" ? (
              <>
                <div className={styles.pWord}>{word}</div>
                {withContext && (
                  <div className={styles.pCard}>
                    <span>{tr("语境句")}</span>
                    <i />
                  </div>
                )}
                {chosen.slice(0, 5).map((c) => (
                  <div key={c.id} className={styles.pCard}>
                    <span>{options.dictNames !== false ? c.name : ""}</span>
                    <i />
                    <i />
                  </div>
                ))}
              </>
            ) : (
              <>
                <div className={styles.pTitle}>{word}</div>
                {withContext && (
                  <div className={styles.pSection}>
                    <span>{tr("我的语境")}</span>
                    <i />
                  </div>
                )}
                {chosen.slice(0, 5).map((c, i) => (
                  <div key={c.id} className={styles.pSection}>
                    <span>{options.dictNames !== false ? `${String(i + 1).padStart(2, "0")} ${c.name}` : ""}</span>
                    <i />
                    <i />
                  </div>
                ))}
              </>
            )}
          </div>
        </aside>
      </div>

      <footer className={styles.foot}>
        {phase.kind === "done" ? (
          <>
            <span className={styles.status} role="status">
              {tr("已导出到 {path}", { path: phase.path })}
            </span>
            <button type="button" className={styles.ghost} onClick={() => void revealItemInDir(phase.path)}>
              {tr("在文件夹中显示")}
            </button>
            <button type="button" className={styles.primary} onClick={onClose}>
              {tr("完成")}
            </button>
          </>
        ) : (
          <>
            <label className={styles.name}>
              {tr("文件名")}
              <input value={name} onChange={(e) => setName(e.target.value)} spellCheck={false} />
              <span>.{ext}</span>
            </label>
            {phase.kind === "error" && (
              <span className={styles.error} role="alert">
                {phase.message}
              </span>
            )}
            <button type="button" className={styles.ghost} onClick={onClose}>
              {tr("取消")}
            </button>
            <button type="button" className={styles.primary} disabled={!canExport || phase.kind === "busy"} onClick={() => void doExport()}>
              {phase.kind === "busy" ? tr("正在导出…") : tr("导出 {format}", { format: ext.toUpperCase() })}
              <kbd>Ctrl+Enter</kbd>
            </button>
          </>
        )}
      </footer>
    </dialog>
  );
}
