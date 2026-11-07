// 全书预习包（阅读 › 导出 › 全书预习包）：整本书按词汇量门槛挑出生词，先看每档有多少个再选；
// 生成一份 PDF / HTML：卷首概况、阅读路线表（每章篇幅、预计用时、生词密度）、按章或按字母的生词表（原句 + 词典释义）、字母索引。
// 第一期只用本机数据：生词统计、本地词典；导读、分章提要这类要联网的内容以后再加。

import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { useEffect, useRef, useState } from "react";
import { ExportStyleOptions, PaperOptions } from "../../components/ExportStyleOptions";
import { CloseIcon } from "../../components/icons";
import { prepareEntry } from "../../dict-render/prepare";
import { buildStudyBody, duration, type StudyExportWord } from "../../export-templates/study";
import { exportTheme, isoDate, safeFileName, type ExportStyle, type Paper } from "../../export-templates/template";
import { inlineEntry } from "../../export/resources";
import { runExport } from "../../export/run";
import { getLang, tr } from "../../i18n";
import { firstEntry, senseGlosses, type SenseGloss } from "../../ipc/lookup";
import { saveScannedWords } from "../../ipc/scan";
import { bookStudyList, bookStudyPreview, type Scope, type StudyOrder, type StudyPreview } from "../../ipc/study";
import { useDicts } from "../../stores/dicts";
import { useSettings } from "../../stores/settings";
import { useAppliedTheme } from "../../theme/useApplyTheme";
import dlg from "../../components/ExportDialog.module.css";
import styles from "./BookStudyDialog.module.css";

interface Props {
  open: boolean;
  onClose: () => void;
  bookId: number;
  /** 阅读页当前的词汇量：门槛默认选它 */
  level: number;
}

interface Opts {
  minCount: number;
  includeSaved: boolean;
  /** 范围里书前书后的附加内容（前言、导读、注释…）也算 */
  includeExtras: boolean;
  order: StudyOrder;
  /** 释义先查的词典；空 = 按生词本设置和配置组顺序 */
  dict: string;
  gloss: boolean;
  sentence: boolean;
  definition: boolean;
  examples: boolean;
  phonetics: boolean;
  roadmap: boolean;
  index: boolean;
  save: boolean;
}

const PREF_KEY = "lens.reader.study";
const DEFAULT_OPTS: Opts = {
  minCount: 1,
  includeSaved: false,
  includeExtras: false,
  order: "chapter",
  dict: "",
  gloss: true,
  sentence: true,
  definition: true,
  examples: false,
  phonetics: true,
  roadmap: true,
  index: true,
  save: false,
};
const GLOSS_BATCH = 100;
/** 带完整词典释义超过这么多词时提醒：PDF 会很长 */
const LONG_LIST = 400;
/** 上千个词的 PDF 排版、打印要几分钟 */
const EXPORT_TIMEOUT_MS = 15 * 60_000;

function readPrefs(): Opts {
  try {
    const v = JSON.parse(localStorage.getItem(PREF_KEY) ?? "null");
    if (v && typeof v === "object") return { ...DEFAULT_OPTS, ...v };
  } catch {
    // 记不住只是回到默认值
  }
  return DEFAULT_OPTS;
}

function scopeOf(range: { from: number; to: number } | null, includeExtras: boolean): Scope {
  return { from: range?.from ?? null, to: range?.to ?? null, includeExtras };
}

type Phase = { kind: "idle" } | { kind: "busy"; note: string } | { kind: "done"; path: string; words: number } | { kind: "error"; message: string };

export function BookStudyDialog({ open, onClose, bookId, level: readerLevel }: Props) {
  const dialog = useRef<HTMLDialogElement>(null);
  const prefs = useSettings((s) => s.settings?.export);
  const accent = useSettings((s) => s.settings?.appearance.accent) ?? "blue";
  const update = useSettings((s) => s.update);
  const appliedTheme = useAppliedTheme((s) => s.theme) ?? "paper";
  const dicts = useDicts((s) => s.dicts).filter((d) => d.kind !== "online" && d.loaded);

  const [opts, setOpts] = useState<Opts>(DEFAULT_OPTS);
  const [level, setLevel] = useState(8000);
  const [preview, setPreview] = useState<StudyPreview | null>(null);
  /** 预习范围（书里的章节序号）；null = 默认范围，每本书不一样，不记住 */
  const [range, setRange] = useState<{ from: number; to: number } | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [format, setFormat] = useState<"pdf" | "html">("pdf");
  const [style, setStyle] = useState<ExportStyle>("paperWhite");
  const [paper, setPaper] = useState<Paper>("a4");
  const [name, setName] = useState("");
  const [phase, setPhase] = useState<Phase>({ kind: "idle" });
  const cancelled = useRef(false);

  useEffect(() => {
    const d = dialog.current;
    if (!d) return;
    if (open && !d.open) {
      setOpts(readPrefs());
      setLevel(readerLevel);
      setRange(null);
      setPreview(null);
      setPreviewError(null);
      if (prefs) {
        setStyle(prefs.style);
        setPaper(prefs.paper);
      }
      setPhase({ kind: "idle" });
      d.showModal();
    }
    if (!open && d.open) d.close();
    // 只在打开那一刻初始化
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  // 第一次打开要把整本书扫一遍（《白鲸》约 2 秒），之后换门槛、范围直接用缓存
  useEffect(() => {
    if (!open) return;
    let alive = true;
    setPreviewError(null);
    bookStudyPreview(bookId, opts.minCount, scopeOf(range, opts.includeExtras)).then(
      (p) => {
        if (!alive) return;
        setPreview(p);
        setName((n) => n || `${safeFileName(p.title)}_${tr("预习")}_${isoDate(new Date())}`);
        setLevel((cur) => (p.levels.some((l) => l.level === cur) ? cur : 8000));
      },
      (e) => alive && setPreviewError(String(e)),
    );
    return () => {
      alive = false;
    };
  }, [open, bookId, opts.minCount, opts.includeExtras, range]);

  const set = <K extends keyof Opts>(k: K, v: Opts[K]) => setOpts((o) => ({ ...o, [k]: v }));
  const extrasInRange = preview ? preview.parts.filter((p) => p.extra && p.idx >= preview.from && p.idx <= preview.to).map((p) => p.title) : [];
  const chosen = preview?.levels.find((l) => l.level === level) ?? null;
  const count = chosen ? chosen.words + (opts.includeSaved ? chosen.saved : 0) : 0;
  const busy = phase.kind === "busy";

  const generate = async () => {
    if (!preview || busy) return;
    const fileName = `${safeFileName(name.replace(/\.(pdf|html?)$/i, "")) || "lens"}.${format}`;
    const path = await saveDialog({
      title: tr("导出全书预习包"),
      defaultPath: prefs?.dir ? `${prefs.dir}\\${fileName}` : fileName,
      filters: [format === "pdf" ? { name: "PDF", extensions: ["pdf"] } : { name: "HTML", extensions: ["html"] }],
    });
    if (!path) return;
    cancelled.current = false;
    const stop = () => {
      if (cancelled.current) throw new Error(tr("已停止"));
    };
    try {
      setPhase({ kind: "busy", note: tr("正在挑选生词…") });
      const list = await bookStudyList(bookId, level, opts.minCount, opts.includeSaved, scopeOf(range, opts.includeExtras), opts.order);
      if (list.words.length === 0) throw new Error(tr("这个门槛下没有生词，换一档词汇量试试"));

      // 词性和各义项的中文：先查选的释义词典，再按配置组顺序查别的双解词典；原形查不到时查书里的写法
      const glosses = new Map<string, SenseGloss>();
      for (let i = 0; i < list.words.length; i += GLOSS_BATCH) {
        stop();
        setPhase({ kind: "busy", note: tr("正在取简释 {i} / {n}", { i: Math.min(i + GLOSS_BATCH, list.words.length), n: list.words.length }) });
        const batch = await senseGlosses(
          list.words.slice(i, i + GLOSS_BATCH).map((w) => ({ lemma: w.lemma, forms: w.forms })),
          opts.dict || null,
        ).catch(() => []);
        for (const g of batch) glosses.set(g.word, g);
      }

      const rows: StudyExportWord[] = [];
      for (const [i, w] of list.words.entries()) {
        stop();
        let entry: StudyExportWord["entry"] = null;
        if (opts.definition) {
          if (i % 10 === 0) setPhase({ kind: "busy", note: tr("正在查词典 {i} / {n}", { i: i + 1, n: list.words.length }) });
          const r = await firstEntry(w.lemma, opts.dict || null, w.forms).catch(() => null);
          if (r) {
            const html = r.entries.map((e) => prepareEntry(e.body, r.dictId).html);
            entry = { name: r.name, html: format === "html" ? await Promise.all(html.map(inlineEntry)) : html };
          }
        }
        const g = glosses.get(w.lemma);
        rows.push({
          lemma: w.lemma,
          surface: w.sentence ? w.sentence.slice(w.hlStart, w.hlEnd) : (w.forms[0] ?? w.lemma),
          status: w.status,
          pos: g?.pos ?? null,
          gloss: opts.gloss ? (g?.senses[0] ?? null) : null,
          senses: opts.gloss ? g?.senses : undefined,
          sentence: w.sentence,
          hlStart: w.hlStart,
          hlEnd: w.hlEnd,
          entry,
          count: w.count,
          chapter: w.chapter,
        });
      }

      if (opts.save) {
        stop();
        setPhase({ kind: "busy", note: tr("正在收进生词本…") });
        const items = rows
          .filter((r) => r.status === "new")
          .map((r) => ({ lemma: r.lemma, surface: r.surface, sentence: r.sentence, hlStart: r.hlStart, hlEnd: r.hlEnd, brief: r.gloss }));
        await saveScannedWords(items, preview.title, [preview.title]);
      }

      stop();
      setPhase({ kind: "busy", note: tr("正在排版，词多时要几分钟…") });
      const body = buildStudyBody({
        title: preview.title,
        author: preview.author,
        words: rows,
        chapters: list.chapters,
        order: opts.order,
        style,
        date: new Date(),
        level,
        options: { sentence: opts.sentence, definition: opts.definition, examples: opts.examples, phonetics: opts.phonetics, roadmap: opts.roadmap, index: opts.index },
        stats: { total: preview.total, minutes: preview.minutes, coverage: chosen?.coverage ?? 1 },
      });
      await runExport({
        jobId: crypto.randomUUID(),
        format,
        body,
        title: `${preview.title} · ${tr("读前预习")}`,
        theme: exportTheme(style, appliedTheme),
        accent,
        path,
        paper,
        lang: getLang(),
        outline: format === "pdf",
        timeoutMs: EXPORT_TIMEOUT_MS,
      });
      try {
        localStorage.setItem(PREF_KEY, JSON.stringify(opts));
      } catch {
        // 同上
      }
      update({ export: { style, paper, dir: path.replace(/[\\/][^\\/]*$/, "") } });
      setPhase({ kind: "done", path, words: rows.length });
    } catch (e) {
      setPhase({ kind: "error", message: String(e instanceof Error ? e.message : e) });
    }
  };

  return (
    <dialog
      ref={dialog}
      className={dlg.dialog}
      onClose={onClose}
      onCancel={(e) => {
        // 生成中按 Esc 不关窗：先停下
        if (busy) {
          e.preventDefault();
          cancelled.current = true;
        }
      }}
      aria-labelledby="book-study-title"
    >
      <header className={dlg.head}>
        <h2 id="book-study-title">{tr("全书预习包")}</h2>
        <span className={dlg.word}>{preview?.title ?? ""}</span>
        <button type="button" className={dlg.iconBtn} aria-label={tr("关闭")} onClick={onClose} disabled={busy}>
          <CloseIcon />
        </button>
      </header>

      <div className={dlg.content}>
        <div className={dlg.form}>
          {previewError ? (
            <p className={dlg.error} role="alert">
              {previewError}
            </p>
          ) : !preview ? (
            <p className={styles.loading} role="status">
              {tr("正在分析全书的生词…")}
            </p>
          ) : (
            <>
              <p className={styles.summary}>
                {[
                  preview.author,
                  tr("{n} 部分", { n: preview.chapters }),
                  tr("{n} 词", { n: preview.total.toLocaleString() }),
                  tr("不同的词 {n} 个", { n: preview.unique.toLocaleString() }),
                  tr("预计阅读 {t}", { t: duration(preview.minutes) }),
                ]
                  .filter(Boolean)
                  .join(" · ")}
              </p>

              <fieldset>
                <legend>
                  {tr("范围")} <small>{tr("默认是正文：跳过前言、注释和书末出版社附赠的内容")}</small>
                </legend>
                <div className={dlg.selects}>
                  <label>
                    <span>{tr("从")}</span>
                    <select value={preview.from} onChange={(e) => setRange({ from: Number(e.target.value), to: preview.to })}>
                      {preview.parts.map((p) => (
                        <option key={p.idx} value={p.idx}>
                          {p.title}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label>
                    <span>{tr("到")}</span>
                    <select value={preview.to} onChange={(e) => setRange({ from: preview.from, to: Number(e.target.value) })}>
                      {preview.parts.map((p) => (
                        <option key={p.idx} value={p.idx}>
                          {p.title}
                        </option>
                      ))}
                    </select>
                  </label>
                </div>
              </fieldset>

              <fieldset>
                <legend>
                  {tr("生词门槛")} <small>{tr(preview.source === "coca" ? "按 COCA 词频" : "按内置词频表")}</small>
                </legend>
                <div className={styles.levels} role="radiogroup" aria-label={tr("生词门槛")}>
                  {preview.levels.map((l) => (
                    <label key={l.level} className={styles.level} data-checked={level === l.level || undefined}>
                      <input type="radio" name="study-level" checked={level === l.level} onChange={() => setLevel(l.level)} />
                      <span>{tr("常用 {n} 词以外", { n: l.level.toLocaleString() })}</span>
                      <strong>{tr("{n} 个", { n: l.words.toLocaleString() })}</strong>
                      <small>{tr("认识 {pct}%", { pct: (l.coverage * 100).toFixed(1) })}</small>
                    </label>
                  ))}
                </div>
                <div className={dlg.selects}>
                  <label>
                    <span>{tr("全书至少出现")}</span>
                    <select value={opts.minCount} onChange={(e) => set("minCount", Number(e.target.value))}>
                      <option value={1}>{tr("1 次（全部）")}</option>
                      <option value={2}>{tr("2 次")}</option>
                      <option value={3}>{tr("3 次")}</option>
                      <option value={5}>{tr("5 次")}</option>
                    </select>
                  </label>
                </div>
                <div className={dlg.checks}>
                  <label>
                    <input type="checkbox" checked={opts.includeSaved} onChange={(e) => set("includeSaved", e.target.checked)} />
                    {tr("已收藏的词也列进来（{n} 个）", { n: chosen?.saved ?? 0 })}
                  </label>
                  {extrasInRange.length > 0 && (
                    <label title={extrasInRange.join("\n")}>
                      <input type="checkbox" checked={opts.includeExtras} onChange={(e) => set("includeExtras", e.target.checked)} />
                      {tr("范围里的附加内容也算（{n} 部分：{names}）", { n: extrasInRange.length, names: extrasInRange.slice(0, 3).join("、") + (extrasInRange.length > 3 ? "…" : "") })}
                    </label>
                  )}
                </div>
                <p className={styles.note}>{tr("标过「认识」的词不算；句中大写的人名地名不算。只出现一次的生僻词收益低，词太多时可以要求至少出现 2 次。")}</p>
              </fieldset>

              <fieldset>
                <legend>{tr("生词表")}</legend>
                <div className={dlg.formats}>
                  {(
                    [
                      ["chapter", "按章节先后", "每章列出这一章第一次出现的词，边预习边读"],
                      ["alpha", "按字母", "A–Z 排列，标出第一次出现在哪一章"],
                    ] as const
                  ).map(([o, label, desc]) => (
                    <label key={o} className={dlg.format} data-checked={opts.order === o || undefined}>
                      <input type="radio" name="study-order" checked={opts.order === o} onChange={() => set("order", o)} />
                      <span>
                        <strong>{tr(label)}</strong>
                        <small>{tr(desc)}</small>
                      </span>
                    </label>
                  ))}
                </div>
                <div className={dlg.selects}>
                  <label>
                    <span>{tr("释义词典")}</span>
                    <select value={opts.dict} onChange={(e) => set("dict", e.target.value)} disabled={!opts.definition}>
                      <option value="">{tr("自动（生词本设置的词典优先）")}</option>
                      {dicts.map((d) => (
                        <option key={d.id} value={d.id}>
                          {d.name}
                        </option>
                      ))}
                    </select>
                  </label>
                </div>
                <div className={dlg.checks}>
                  <label>
                    <input type="checkbox" checked={opts.sentence} onChange={(e) => set("sentence", e.target.checked)} />
                    {tr("书中原句")}
                  </label>
                  <label title={tr("各义项的中文，取自选的释义词典；它不是双解词典或没收这个词时，按配置组顺序找别的双解词典")}>
                    <input type="checkbox" checked={opts.gloss} onChange={(e) => set("gloss", e.target.checked)} />
                    {tr("中文释义（各义项）")}
                  </label>
                  <label>
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
                </div>
                {opts.definition && count > LONG_LIST && (
                  <p className={styles.note}>{tr("{n} 个词带完整词典释义，PDF 会很长，生成要几分钟。只要中文简释可以取消「词典释义」。", { n: count.toLocaleString() })}</p>
                )}
              </fieldset>

              <fieldset>
                <legend>{tr("还包括")}</legend>
                <div className={dlg.checks}>
                  <label>
                    <input type="checkbox" checked={opts.roadmap} onChange={(e) => set("roadmap", e.target.checked)} />
                    {tr("阅读路线表（每章篇幅、预计用时、生词密度）")}
                  </label>
                  <label>
                    <input type="checkbox" checked={opts.index} disabled={opts.order !== "chapter"} onChange={(e) => set("index", e.target.checked)} />
                    {tr("字母索引")}
                  </label>
                  <label>
                    <input type="checkbox" checked={opts.save} onChange={(e) => set("save", e.target.checked)} />
                    {tr("同时收进生词本（带书中原句，标签为书名）")}
                  </label>
                </div>
              </fieldset>

              <fieldset>
                <legend>{tr("格式")}</legend>
                <div className={dlg.formats}>
                  {(["pdf", "html"] as const).map((f) => (
                    <label key={f} className={dlg.format} data-checked={format === f || undefined}>
                      <input type="radio" name="study-format" checked={format === f} onChange={() => setFormat(f)} />
                      <span>
                        <strong>{f.toUpperCase()}</strong>
                        <small>{tr(f === "pdf" ? "带按章节跳转的书签" : "单个文件，浏览器直接打开")}</small>
                      </span>
                    </label>
                  ))}
                </div>
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
              </fieldset>
            </>
          )}
        </div>
      </div>

      <footer className={dlg.foot}>
        {phase.kind === "done" ? (
          <>
            <span className={dlg.status} role="status">
              {tr("已导出 {n} 个词到 {path}", { n: phase.words, path: phase.path })}
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
              <input value={name} onChange={(e) => setName(e.target.value)} spellCheck={false} disabled={busy} />
              <span>.{format}</span>
            </label>
            {phase.kind === "error" && (
              <span className={dlg.error} role="alert">
                {phase.message}
              </span>
            )}
            {busy && (
              <span className={dlg.status} role="status">
                {phase.note}
              </span>
            )}
            {busy ? (
              <button type="button" className={dlg.ghost} onClick={() => (cancelled.current = true)}>
                {tr("停止")}
              </button>
            ) : (
              <button type="button" className={dlg.ghost} onClick={onClose}>
                {tr("取消")}
              </button>
            )}
            <button type="button" className={dlg.primary} disabled={!preview || busy || count === 0} onClick={() => void generate()}>
              {busy ? tr("正在生成…") : tr("生成 {n} 个词的预习包", { n: count.toLocaleString() })}
            </button>
          </>
        )}
      </footer>
    </dialog>
  );
}
