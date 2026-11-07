// 文章导出对话框（阅读 › 导出）：PDF / HTML；书页 / 旁注 / 卡片三种版式；样式、纸张和单词导出共用。
// 前半是原文（生词标注、编号），后半是词汇表：单词、词性、中文释义、文中原句、词典释义（设置 › 生词本选的那部词典优先）。

import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { useEffect, useRef, useState } from "react";
import { ExportStyleOptions, PaperOptions } from "../../components/ExportStyleOptions";
import { CloseIcon } from "../../components/icons";
import { prepareEntry } from "../../dict-render/prepare";
import { buildArticleBody, type ArticleLayout, type ArticleWord } from "../../export-templates/article";
import { exportTheme, isoDate, safeFileName, type ExportStyle, type Paper } from "../../export-templates/template";
import { inlineEntry } from "../../export/resources";
import { runExport } from "../../export/run";
import { getLang, tr } from "../../i18n";
import { firstEntry, wordGlosses } from "../../ipc/lookup";
import type { ScanResult } from "../../ipc/scan";
import { useSettings } from "../../stores/settings";
import { useAppliedTheme } from "../../theme/useApplyTheme";
import dlg from "../../components/ExportDialog.module.css";
import styles from "./ArticleExportDialog.module.css";

interface Props {
  open: boolean;
  onClose: () => void;
  title: string;
  scan: ScanResult | null;
  /** 阅读视图里排好版、包好词的正文 HTML */
  getHtml: () => string;
  /** 电子书：改为生成整本书的预习包 */
  onStudy?: () => void;
}

interface Opts {
  newWords: boolean;
  savedWords: boolean;
  sentence: boolean;
  definition: boolean;
  examples: boolean;
  phonetics: boolean;
  meta: boolean;
}

const LAYOUTS: Array<[ArticleLayout, string, string]> = [
  ["book", "书页", "下划线 + 上标编号，词汇表放在文末"],
  ["margin", "旁注", "每段右边注出本段的生词和释义"],
  ["cards", "卡片", "荧光笔标注，词汇表排成卡片"],
];

const PREF_KEY = "lens.reader.export";
const DEFAULT_OPTS: Opts = { newWords: true, savedWords: true, sentence: true, definition: true, examples: false, phonetics: true, meta: true };

function readPrefs(): { layout: ArticleLayout; opts: Opts } {
  try {
    const v = JSON.parse(localStorage.getItem(PREF_KEY) ?? "null");
    if (v && LAYOUTS.some(([l]) => l === v.layout)) return { layout: v.layout, opts: { ...DEFAULT_OPTS, ...v.opts } };
  } catch {
    // 记不住只是回到默认值
  }
  return { layout: "book", opts: DEFAULT_OPTS };
}

type Phase = { kind: "idle" } | { kind: "busy"; note: string } | { kind: "done"; path: string } | { kind: "error"; message: string };

export function ArticleExportDialog({ open, onClose, title, scan, getHtml, onStudy }: Props) {
  const dialog = useRef<HTMLDialogElement>(null);
  const prefs = useSettings((s) => s.settings?.export);
  const accent = useSettings((s) => s.settings?.appearance.accent) ?? "blue";
  const update = useSettings((s) => s.update);
  const appliedTheme = useAppliedTheme((s) => s.theme) ?? "paper";

  const [format, setFormat] = useState<"pdf" | "html">("pdf");
  const [layout, setLayout] = useState<ArticleLayout>("book");
  const [opts, setOpts] = useState<Opts>(DEFAULT_OPTS);
  const [style, setStyle] = useState<ExportStyle>("paperWhite");
  const [paper, setPaper] = useState<Paper>("a4");
  const [name, setName] = useState("");
  const [phase, setPhase] = useState<Phase>({ kind: "idle" });

  useEffect(() => {
    const d = dialog.current;
    if (!d) return;
    if (open && !d.open) {
      const p = readPrefs();
      setLayout(p.layout);
      setOpts(p.opts);
      if (prefs) {
        setStyle(prefs.style);
        setPaper(prefs.paper);
      }
      setName(`${safeFileName(title)}_${isoDate(new Date())}`);
      setPhase({ kind: "idle" });
      d.showModal();
    }
    if (!open && d.open) d.close();
    // 只在打开那一刻初始化
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  const all = scan?.words ?? [];
  const nNew = all.filter((w) => w.status === "new").length;
  const nSaved = all.filter((w) => w.status === "saved").length;
  const words = all.filter((w) => (w.status === "new" && opts.newWords) || (w.status === "saved" && opts.savedWords));
  const set = (k: keyof Opts, v: boolean) => setOpts((o) => ({ ...o, [k]: v }));
  const ext = format;

  const doExport = async () => {
    if (!scan || phase.kind === "busy") return;
    const fileName = `${safeFileName(name.replace(/\.(pdf|html?)$/i, "")) || "lens"}.${ext}`;
    const path = await saveDialog({
      title: tr("导出文章"),
      defaultPath: prefs?.dir ? `${prefs.dir}\\${fileName}` : fileName,
      filters: [format === "pdf" ? { name: "PDF", extensions: ["pdf"] } : { name: "HTML", extensions: ["html"] }],
    });
    if (!path) return;
    try {
      setPhase({ kind: "busy", note: tr("正在取释义…") });
      const glosses = new Map((await wordGlosses(words.map((w) => w.lemma)).catch(() => [])).map((g) => [g.word, g]));
      const rows: ArticleWord[] = [];
      for (const [i, w] of words.entries()) {
        let entry: ArticleWord["entry"] = null;
        if (opts.definition) {
          setPhase({ kind: "busy", note: tr("正在查词典 {i} / {n}", { i: i + 1, n: words.length }) });
          const r = await firstEntry(w.lemma).catch(() => null);
          if (r) {
            const html = r.entries.map((e) => prepareEntry(e.body, r.dictId).html);
            // HTML 要单文件可用：词典 CSS 与图片内联
            entry = { name: r.name, html: format === "html" ? await Promise.all(html.map(inlineEntry)) : html };
          }
        }
        const g = glosses.get(w.lemma);
        rows.push({
          lemma: w.lemma,
          surface: w.sentence ? w.sentence.slice(w.hlStart, w.hlEnd) : (w.forms[0] ?? w.lemma),
          status: w.status === "saved" ? "saved" : "new",
          pos: g?.pos ?? null,
          gloss: g?.gloss ?? null,
          sentence: w.sentence,
          hlStart: w.hlStart,
          hlEnd: w.hlEnd,
          entry,
        });
      }
      setPhase({ kind: "busy", note: tr("正在排版…") });
      const body = buildArticleBody({
        title,
        articleHtml: getHtml(),
        words: rows,
        layout,
        style,
        date: new Date(),
        options: { sentence: opts.sentence, definition: opts.definition, examples: opts.examples, phonetics: opts.phonetics, meta: opts.meta },
        stats: { total: scan.total, coverage: scan.coverage },
      });
      await runExport({
        jobId: crypto.randomUUID(),
        format,
        body,
        title: `${title} · ${tr("Lens 词镜")}`,
        theme: exportTheme(style, appliedTheme),
        accent,
        path,
        paper,
        lang: getLang(),
      });
      try {
        localStorage.setItem(PREF_KEY, JSON.stringify({ layout, opts }));
      } catch {
        // 同上
      }
      update({ export: { style, paper, dir: path.replace(/[\\/][^\\/]*$/, "") } });
      setPhase({ kind: "done", path });
    } catch (e) {
      setPhase({ kind: "error", message: String(e instanceof Error ? e.message : e) });
    }
  };

  return (
    <dialog
      ref={dialog}
      className={dlg.dialog}
      onClose={onClose}
      aria-labelledby="article-export-title"
      onKeyDown={(e) => {
        if (e.ctrlKey && e.key === "Enter") {
          e.preventDefault();
          void doExport();
        }
      }}
    >
      <header className={dlg.head}>
        <h2 id="article-export-title">{tr("导出文章")}</h2>
        <span className={dlg.word}>{title}</span>
        {onStudy && (
          <button type="button" className={dlg.ghost} onClick={onStudy} title={tr("整本书的生词表、阅读路线表，读之前先预习")}>
            {tr("全书预习包…")}
          </button>
        )}
        <button type="button" className={dlg.iconBtn} aria-label={tr("关闭")} onClick={onClose}>
          <CloseIcon />
        </button>
      </header>

      <div className={dlg.content}>
        <div className={dlg.form}>
          <fieldset>
            <legend>{tr("版式")}</legend>
            <div className={styles.layouts}>
              {LAYOUTS.map(([l, label, desc]) => (
                <label key={l} className={styles.layout} data-checked={layout === l || undefined}>
                  <input type="radio" name="article-layout" checked={layout === l} onChange={() => setLayout(l)} />
                  <Thumb layout={l} />
                  <strong>{tr(label)}</strong>
                  <small>{tr(desc)}</small>
                </label>
              ))}
            </div>
          </fieldset>

          <fieldset>
            <legend>
              {tr("标注的词")} <small>{tr("共 {n} 个", { n: words.length })}</small>
            </legend>
            <div className={dlg.checks}>
              <label>
                <input type="checkbox" checked={opts.newWords} onChange={(e) => set("newWords", e.target.checked)} />
                {tr("生词 {n}", { n: nNew })}
              </label>
              <label>
                <input type="checkbox" checked={opts.savedWords} onChange={(e) => set("savedWords", e.target.checked)} />
                {tr("已收藏 {n}", { n: nSaved })}
              </label>
            </div>
          </fieldset>

          <fieldset>
            <legend>{tr("词汇表")}</legend>
            <div className={dlg.checks}>
              <label>
                <input type="checkbox" checked={opts.sentence} onChange={(e) => set("sentence", e.target.checked)} />
                {tr("文中原句")}
              </label>
              <label title={tr("设置 › 生词本里选的词典优先，否则按当前配置组的顺序")}>
                <input type="checkbox" checked={opts.definition} onChange={(e) => set("definition", e.target.checked)} />
                {tr("词典释义")}
              </label>
              <label>
                <input type="checkbox" checked={opts.examples} disabled={!opts.definition} onChange={(e) => set("examples", e.target.checked)} />
                {tr("词典例句")}
              </label>
              <label>
                <input type="checkbox" checked={opts.phonetics} disabled={!opts.definition} onChange={(e) => set("phonetics", e.target.checked)} />
                {tr("音标")}
              </label>
              <label>
                <input type="checkbox" checked={opts.meta} onChange={(e) => set("meta", e.target.checked)} />
                {tr("卷首统计")}
              </label>
            </div>
          </fieldset>

          <fieldset>
            <legend>{tr("格式")}</legend>
            <div className={dlg.formats}>
              {(["pdf", "html"] as const).map((f) => (
                <label key={f} className={dlg.format} data-checked={format === f || undefined}>
                  <input type="radio" name="article-format" checked={format === f} onChange={() => setFormat(f)} />
                  <span>
                    <strong>{f.toUpperCase()}</strong>
                    <small>{tr(f === "pdf" ? "打印、归档、发给别人" : "单个文件，浏览器直接打开")}</small>
                  </span>
                </label>
              ))}
            </div>
          </fieldset>

          <div className={dlg.selects}>
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
        </div>
      </div>

      <footer className={dlg.foot}>
        {phase.kind === "done" ? (
          <>
            <span className={dlg.status} role="status">
              {tr("已导出到 {path}", { path: phase.path })}
            </span>
            <button type="button" className={dlg.ghost} onClick={() => void revealItemInDir(phase.path)}>
              {tr("在文件夹中显示")}
            </button>
            <button type="button" className={dlg.primary} onClick={onClose}>
              {tr("完成")}
            </button>
          </>
        ) : (
          <>
            <label className={dlg.name}>
              {tr("文件名")}
              <input value={name} onChange={(e) => setName(e.target.value)} spellCheck={false} />
              <span>.{ext}</span>
            </label>
            {phase.kind === "error" && (
              <span className={dlg.error} role="alert">
                {phase.message}
              </span>
            )}
            {phase.kind === "busy" && (
              <span className={dlg.status} role="status">
                {phase.note}
              </span>
            )}
            <button type="button" className={dlg.ghost} onClick={onClose}>
              {tr("取消")}
            </button>
            <button type="button" className={dlg.primary} disabled={!scan || phase.kind === "busy"} onClick={() => void doExport()}>
              {phase.kind === "busy" ? tr("正在导出…") : tr("导出 {format}", { format: ext.toUpperCase() })}
              <kbd>Ctrl+Enter</kbd>
            </button>
          </>
        )}
      </footer>
    </dialog>
  );
}

/** 版式缩略图：线框示意，颜色取主题变量 */
function Thumb({ layout }: { layout: ArticleLayout }) {
  const lines = (n: number, mark: number[] = []) =>
    Array.from({ length: n }, (_, i) => <i key={i} data-mark={mark.includes(i) || undefined} />);
  return (
    <span className={styles.thumb} data-layout={layout} aria-hidden="true">
      <b />
      {layout === "margin" ? (
        <span className={styles.tRow}>
          <span className={styles.tText}>{lines(5, [1, 3])}</span>
          <span className={styles.tNote}>
            <i />
            <i />
          </span>
        </span>
      ) : (
        <span className={styles.tText}>{lines(5, [1, 3])}</span>
      )}
      <span className={styles.tGloss}>
        {layout === "cards" ? (
          <>
            <em />
            <em />
          </>
        ) : (
          <>
            <i />
            <i />
          </>
        )}
      </span>
    </span>
  );
}
