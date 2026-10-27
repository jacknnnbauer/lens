// 生词本导出对话框：Anki 牌组包（.apkg，带卡片模板和发音，双击导入）、Anki 文本（.txt）或 CSV。按当前筛选导出。

import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { useEffect, useRef, useState } from "react";
import { exportAnkiPackage, exportWordbook, type AnkiOptions, type WordStatus } from "../../ipc/words";
import styles from "./AnkiExportDialog.module.css";
import { tr } from "../../i18n";

type Format = "apkg" | "anki" | "csv";
type Phase = { kind: "idle" } | { kind: "busy" } | { kind: "done"; path: string; message: string } | { kind: "error"; message: string };

const PREFS_KEY = "lens.ankiExport";

interface Props {
  open: boolean;
  onClose: () => void;
  query: { status?: WordStatus; tag?: string; search?: string };
}

function defaults(): AnkiOptions & { format: Format } {
  const base = { format: "apkg" as Format, deck: tr("词镜 Lens 生词本"), recall: true, context: true, audio: true, accent: "us" as const };
  try {
    const saved = JSON.parse(localStorage.getItem(PREFS_KEY) ?? "null");
    return saved && typeof saved === "object" ? { ...base, ...saved } : base;
  } catch {
    return base;
  }
}

export function AnkiExportDialog({ open, onClose, query }: Props) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [opts, setOpts] = useState(defaults);
  const [phase, setPhase] = useState<Phase>({ kind: "idle" });

  useEffect(() => {
    const d = dialog.current;
    if (!d) return;
    if (open && !d.open) {
      setPhase({ kind: "idle" });
      d.showModal();
    } else if (!open && d.open) {
      d.close();
    }
  }, [open]);

  const patch = (p: Partial<typeof opts>) => {
    const next = { ...opts, ...p };
    setOpts(next);
    try {
      localStorage.setItem(PREFS_KEY, JSON.stringify(next));
    } catch {
      // 记不住选项只是下次回到默认
    }
  };

  const run = async () => {
    const date = new Date().toISOString().slice(0, 10);
    const ext = opts.format === "apkg" ? "apkg" : opts.format === "csv" ? "csv" : "txt";
    const path = await saveDialog({
      title: tr("导出生词本"),
      defaultPath: `Lens-wordbook_${date}.${ext}`,
      filters: [{ name: opts.format === "apkg" ? tr("Anki 牌组包") : opts.format === "csv" ? "CSV（Excel）" : tr("Anki 文本（导入到 Anki）"), extensions: [ext] }],
    });
    if (!path) return;
    setPhase({ kind: "busy" });
    try {
      if (opts.format === "apkg") {
        const { format: _f, ...anki } = opts;
        const r = await exportAnkiPackage(path, anki, query);
        setPhase({
          kind: "done",
          path,
          message: tr("已导出 {w} 个词、{c} 张卡片，{a} 个词带发音。在 Anki 里双击这个文件（或 文件 › 导入）即可。", { w: r.words, c: r.cards, a: r.audio }),
        });
      } else {
        const n = await exportWordbook(path, opts.format, query);
        setPhase({ kind: "done", path, message: tr("已导出 {n} 个词", { n }) });
      }
    } catch (e) {
      setPhase({ kind: "error", message: String(e) });
    }
  };

  const apkg = opts.format === "apkg";
  const noCards = apkg && !opts.recall && !opts.context;

  return (
    <dialog ref={dialog} className={styles.dialog} aria-labelledby="anki-export-title" onClose={onClose} onCancel={onClose}>
      <header className={styles.head}>
        <h2 id="anki-export-title">{tr("导出生词本")}</h2>
        <span className={styles.sub}>{tr("按当前的筛选和搜索")}</span>
      </header>
      <div className={styles.body}>
        <fieldset>
          <legend>{tr("格式")}</legend>
          {(
            [
              ["apkg", tr("Anki 牌组包（.apkg）"), tr("推荐：卡片模板、发音都打包好，双击导入；再次导出同一个词会更新原来的卡片")],
              ["anki", tr("Anki 文本（.txt）"), tr("导入时要自己选笔记类型、对应字段")],
              ["csv", "CSV", tr("Excel 能直接打开")],
            ] as Array<[Format, string, string]>
          ).map(([f, name, desc]) => (
            <label key={f} className={styles.option}>
              <input type="radio" name="anki-format" checked={opts.format === f} onChange={() => patch({ format: f })} />
              <span>
                <b>{name}</b>
                <small>{desc}</small>
              </span>
            </label>
          ))}
        </fieldset>

        {apkg && (
          <>
            <label className={styles.field}>
              <span>{tr("牌组名")}</span>
              <input type="text" value={opts.deck} onChange={(e) => patch({ deck: e.target.value })} />
            </label>
            <fieldset>
              <legend>{tr("卡片")}</legend>
              <label className={styles.check}>
                <input type="checkbox" checked={opts.recall} onChange={(e) => patch({ recall: e.target.checked })} />
                {tr("识记卡：正面单词和发音，背面释义、语境句")}
              </label>
              <label className={styles.check}>
                <input type="checkbox" checked={opts.context} onChange={(e) => patch({ context: e.target.checked })} />
                {tr("语境卡：正面挖空的原句和释义，背面单词（只给有语境句的词生成）")}
              </label>
            </fieldset>
            <fieldset>
              <legend>{tr("发音")}</legend>
              <label className={styles.check}>
                <input type="checkbox" checked={opts.audio} onChange={(e) => patch({ audio: e.target.checked })} />
                {tr("从本地词典里取发音，打包进牌组")}
              </label>
              <label className={styles.inline}>
                {tr("优先")}
                <select value={opts.accent} disabled={!opts.audio} onChange={(e) => patch({ accent: e.target.value as "us" | "uk" })}>
                  <option value="us">{tr("美音")}</option>
                  <option value="uk">{tr("英音")}</option>
                </select>
              </label>
            </fieldset>
          </>
        )}
      </div>
      <footer className={styles.foot}>
        {phase.kind === "done" && (
          <>
            <span className={styles.status} role="status">
              {phase.message}
            </span>
            <button type="button" className={styles.ghost} onClick={() => void revealItemInDir(phase.path)}>
              {tr("打开所在文件夹")}
            </button>
            <button type="button" className={styles.primary} onClick={onClose}>
              {tr("完成")}
            </button>
          </>
        )}
        {phase.kind !== "done" && (
          <>
            {phase.kind === "error" && (
              <span className={styles.error} role="alert">
                {phase.message}
              </span>
            )}
            {noCards && <span className={styles.error}>{tr("至少选一种卡片")}</span>}
            <button type="button" className={styles.ghost} onClick={onClose}>
              {tr("取消")}
            </button>
            <button type="button" className={styles.primary} disabled={phase.kind === "busy" || noCards} onClick={() => void run()}>
              {phase.kind === "busy" ? tr("导出中…") : tr("导出…")}
            </button>
          </>
        )}
      </footer>
    </dialog>
  );
}
