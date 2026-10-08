// 导出词条对话框（SPEC 11、画板 07）：PDF / HTML，卡片式 / 文档式，三种样式，A4 / Letter。

import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { inlineEntry } from "../export/resources";
import { runExport } from "../export/run";
import {
  buildExportBody,
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
import { CloseIcon } from "./icons";

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
  const [options, setOptions] = useState<ExportOptions>({ phonetics: true, examples: true, context: true, sourceDate: true });
  const [selected, setSelected] = useState<Set<string>>(new Set());
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

  const chosen = hits.filter((c) => selected.has(c.id));
  const ext = format === "pdf" ? "pdf" : "html";

  const doExport = async () => {
    if (chosen.length === 0 || phase.kind === "busy") return;
    const fileName = `${safeFileName(name.replace(/\.(pdf|html?)$/i, "")) || "lens"}.${ext}`;
    const path = await saveDialog({
      title: "导出词条",
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
      const theme = style === "inkSaving" ? "eink" : style === "followTheme" ? appliedTheme : "paper";
      await runExport({ jobId: crypto.randomUUID(), format, body, title: `${word} · Lens 词镜`, theme, accent, path, paper });
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
        <h2 id="export-title">导出词条</h2>
        <span className={styles.word}>{word}</span>
        <button type="button" className={styles.iconBtn} aria-label="关闭" onClick={onClose}>
          <CloseIcon />
        </button>
      </header>

      <div className={styles.content}>
        <div className={styles.form}>
          <fieldset>
            <legend>格式</legend>
            <div className={styles.formats}>
              {(["pdf", "html"] as const).map((f) => (
                <label key={f} className={styles.format} data-checked={format === f || undefined}>
                  <input type="radio" name="export-format" checked={format === f} onChange={() => setFormat(f)} />
                  <span>
                    <strong>{f.toUpperCase()}</strong>
                    <small>{f === "pdf" ? "打印、归档、发给别人" : "单个文件，浏览器直接打开"}</small>
                  </span>
                </label>
              ))}
            </div>
          </fieldset>

          <fieldset>
            <legend>
              包含的词典 <small>{chosen.length} / {hits.length} · 按查词顺序</small>
            </legend>
            <div className={styles.dicts}>
              {hits.map((c) => (
                <label key={c.id}>
                  <input type="checkbox" checked={selected.has(c.id)} onChange={() => toggle(c.id)} />
                  {c.name}
                  <small>{c.source === "local" ? "离线" : "在线"}</small>
                </label>
              ))}
            </div>
          </fieldset>

          <div className={styles.selects}>
            <label>
              <span>版式</span>
              <select value={layout} onChange={(e) => setLayout(e.target.value as ExportLayout)}>
                <option value="cards">卡片式（同简洁单栏）</option>
                <option value="document">文档式</option>
              </select>
            </label>
            <label>
              <span>样式</span>
              <select value={style} onChange={(e) => setStyle(e.target.value as ExportStyle)}>
                <option value="paperWhite">纸本（白底打印版）</option>
                <option value="followTheme">跟随当前主题</option>
                <option value="inkSaving">黑白省墨</option>
              </select>
            </label>
            <label>
              <span>纸张</span>
              <select value={paper} disabled={format === "html"} onChange={(e) => setPaper(e.target.value as Paper)}>
                <option value="a4">A4 纵向</option>
                <option value="letter">Letter 纵向</option>
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
              ] as const
            ).map(([k, label]) => (
              <label key={k}>
                <input
                  type="checkbox"
                  checked={options[k]}
                  disabled={k === "context" && !context}
                  onChange={(e) => setOptions((o) => ({ ...o, [k]: e.target.checked }))}
                />
                {label}
              </label>
            ))}
          </div>
        </div>

        <aside className={styles.preview} aria-label="版式示意">
          <div className={styles.previewHead}>示意</div>
          <div className={styles.page} data-layout={layout} data-style={style}>
            {layout === "cards" ? (
              <>
                <div className={styles.pWord}>{word}</div>
                {chosen.slice(0, 5).map((c) => (
                  <div key={c.id} className={styles.pCard}>
                    <span>{c.name}</span>
                    <i />
                    <i />
                  </div>
                ))}
              </>
            ) : (
              <>
                <div className={styles.pTitle}>{word}</div>
                {chosen.slice(0, 5).map((c, i) => (
                  <div key={c.id} className={styles.pSection}>
                    <span>
                      {String(i + 1).padStart(2, "0")} {c.name}
                    </span>
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
              已导出到 {phase.path}
            </span>
            <button type="button" className={styles.ghost} onClick={() => void revealItemInDir(phase.path)}>
              在文件夹中显示
            </button>
            <button type="button" className={styles.primary} onClick={onClose}>
              完成
            </button>
          </>
        ) : (
          <>
            <label className={styles.name}>
              文件名
              <input value={name} onChange={(e) => setName(e.target.value)} spellCheck={false} />
              <span>.{ext}</span>
            </label>
            {phase.kind === "error" && (
              <span className={styles.error} role="alert">
                {phase.message}
              </span>
            )}
            <button type="button" className={styles.ghost} onClick={onClose}>
              取消
            </button>
            <button type="button" className={styles.primary} disabled={chosen.length === 0 || phase.kind === "busy"} onClick={() => void doExport()}>
              {phase.kind === "busy" ? "正在导出…" : `导出 ${ext.toUpperCase()}`}
              <kbd>Ctrl+Enter</kbd>
            </button>
          </>
        )}
      </footer>
    </dialog>
  );
}
