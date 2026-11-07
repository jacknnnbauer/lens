// 全书预习包（阅读 › 导出 › 全书预习包）：整本书按词汇量门槛挑出生词，先看每档有多少个再选；
// 生成一份 PDF / HTML：卷首概况、阅读路线表（每章篇幅、预计用时、生词密度）、按章或按字母的生词表（原句 + 词典释义）、字母索引。
// 第一期只用本机数据：生词统计、本地词典；导读、分章提要这类要联网的内容以后再加。

import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useEffect, useRef, useState } from "react";
import { ExportStyleOptions, PaperOptions } from "../../components/ExportStyleOptions";
import { CloseIcon } from "../../components/icons";
import { duration } from "../../export-templates/study";
import { exportTheme, isoDate, safeFileName, type ExportStyle, type Paper } from "../../export-templates/template";
import { getLang, tr } from "../../i18n";
import { bookGuide, bookGuideAi, bookStudyPreview, type BookGuide, type Scope, type StudyOrder, type StudyPreview } from "../../ipc/study";
import { useDicts } from "../../stores/dicts";
import { useSettings } from "../../stores/settings";
import { useStudyJob } from "../../stores/studyJob";
import { toast } from "../../stores/toast";
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
  /** 最多要多少个词（书里出现次数多的优先）；0 = 不限 */
  limit: number;
  /** 简明单词表（单词、词性、中文）、详细词表（原句、词典释义） */
  simple: boolean;
  detail: boolean;
  /** 读前导读 */
  guide: boolean;
  /** 导读来源：维基百科、AI（按用量计费）、两份都放 */
  guideSource: "wiki" | "ai" | "both";
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
  limit: 0,
  simple: true,
  detail: true,
  guide: true,
  guideSource: "wiki",
};
/** 数量上限的常用档位；别的数在「自定义」里填 */
const LIMITS = [0, 100, 200, 300, 500, 800, 1000];
/** 带完整词典释义超过这么多词时提醒：PDF 会很长 */
const LONG_LIST = 400;

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

type GuideState = { kind: "loading" } | { kind: "found"; guide: BookGuide } | { kind: "none" } | { kind: "error"; message: string };
/** AI 导读：idle = 还没生成（要用户点按钮，花钱） */
type AiGuideState = { kind: "idle" } | { kind: "loading" } | { kind: "found"; guide: BookGuide } | { kind: "error"; message: string };


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
  const [guide, setGuide] = useState<GuideState>({ kind: "loading" });
  const [aiGuide, setAiGuide] = useState<AiGuideState>({ kind: "idle" });
  /** 数量上限选了「自定义」 */
  const [customLimit, setCustomLimit] = useState(false);
  const study = useSettings((s) => s.settings?.study);

  useEffect(() => {
    const d = dialog.current;
    if (!d) return;
    if (open && !d.open) {
      const p = readPrefs();
      setOpts(p);
      setCustomLimit(!LIMITS.includes(p.limit));
      setLevel(readerLevel);
      setRange(null);
      setPreview(null);
      setPreviewError(null);
      if (prefs) {
        setStyle(prefs.style);
        setPaper(prefs.paper);
      }
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

  // AI 导读：只取存过的（不花钱）；没有就等用户点「用 AI 生成」
  const aiProvider = useSettings((s) => s.settings?.ai.provider) ?? "claude";
  useEffect(() => {
    if (!open || !opts.guide || opts.guideSource === "wiki") return;
    let alive = true;
    bookGuideAi(bookId, true).then(
      (g) => alive && setAiGuide(g ? { kind: "found", guide: g } : { kind: "idle" }),
      () => alive && setAiGuide({ kind: "idle" }),
    );
    return () => {
      alive = false;
    };
  }, [open, bookId, opts.guide, opts.guideSource, study?.spoilers]);
  const makeAiGuide = async (force: boolean) => {
    setAiGuide({ kind: "loading" });
    try {
      const g = await bookGuideAi(bookId, false, force);
      setAiGuide(g ? { kind: "found", guide: g } : { kind: "idle" });
    } catch (e) {
      setAiGuide({ kind: "error", message: String(e) });
    }
  };

  // 读前导读：勾选时查一次维基百科（只发书名和作者），剧透、语言改了重新查
  useEffect(() => {
    if (!open || !opts.guide || opts.guideSource === "ai") return;
    let alive = true;
    setGuide({ kind: "loading" });
    bookGuide(bookId).then(
      (g) => alive && setGuide(g ? { kind: "found", guide: g } : { kind: "none" }),
      (e) => alive && setGuide({ kind: "error", message: String(e) }),
    );
    return () => {
      alive = false;
    };
  }, [open, bookId, opts.guide, opts.guideSource, study?.spoilers, study?.guideLang]);

  const set = <K extends keyof Opts>(k: K, v: Opts[K]) => setOpts((o) => ({ ...o, [k]: v }));
  const extrasInRange = preview ? preview.parts.filter((p) => p.extra && p.idx >= preview.from && p.idx <= preview.to).map((p) => p.title) : [];
  const chosen = preview?.levels.find((l) => l.level === level) ?? null;
  const available = chosen ? chosen.words + (opts.includeSaved ? chosen.saved : 0) : 0;
  const count = opts.limit > 0 ? Math.min(opts.limit, available) : available;

  const running = useStudyJob((s) => s.job?.state === "running");
  const startJob = useStudyJob((s) => s.start);

  /** 选好保存位置就交给后台：对话框关掉，右下角显示进度，可以照常查词、阅读 */
  const generate = async () => {
    if (!preview || running) return;
    const fileName = `${safeFileName(name.replace(/\.(pdf|html?)$/i, "")) || "lens"}.${format}`;
    const path = await saveDialog({
      title: tr("导出全书预习包"),
      defaultPath: prefs?.dir ? `${prefs.dir}\\${fileName}` : fileName,
      filters: [format === "pdf" ? { name: "PDF", extensions: ["pdf"] } : { name: "HTML", extensions: ["html"] }],
    });
    if (!path) return;
    const started = startJob({
      bookId,
      title: preview.title,
      author: preview.author,
      level,
      minCount: opts.minCount,
      includeSaved: opts.includeSaved,
      scope: scopeOf(range, opts.includeExtras),
      order: opts.order,
      limit: opts.limit > 0 ? opts.limit : null,
      dict: opts.dict || null,
      gloss: opts.gloss,
      sentence: opts.sentence,
      definition: opts.definition,
      examples: opts.examples,
      phonetics: opts.phonetics,
      roadmap: opts.roadmap,
      index: opts.index,
      simple: opts.simple,
      detail: opts.detail,
      save: opts.save,
      guides: !opts.guide
        ? []
        : [
            ...(opts.guideSource !== "ai" && guide.kind === "found" ? [guide.guide] : []),
            ...(opts.guideSource !== "wiki" && aiGuide.kind === "found" ? [aiGuide.guide] : []),
          ],
      stats: { total: preview.total, minutes: preview.minutes, coverage: chosen?.coverage ?? 1 },
      format,
      style,
      theme: exportTheme(style, appliedTheme),
      accent,
      paper,
      path,
      lang: getLang(),
    });
    if (!started) return;
    try {
      localStorage.setItem(PREF_KEY, JSON.stringify(opts));
    } catch {
      // 记不住只是回到默认值
    }
    update({ export: { style, paper, dir: path.replace(/[\\/][^\\/]*$/, "") } });
    toast(tr("已在后台生成，可以继续查词、阅读"), tr("右下角看进度"));
    onClose();
  };

  return (
    <dialog
      ref={dialog}
      className={dlg.dialog}
      onClose={onClose}
      aria-labelledby="book-study-title"
    >
      <header className={dlg.head}>
        <h2 id="book-study-title">{tr("全书预习包")}</h2>
        <span className={dlg.word}>{preview?.title ?? ""}</span>
        <button type="button" className={dlg.iconBtn} aria-label={tr("关闭")} onClick={onClose}>
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
                  <label title={tr("超过这个数时，留下书里出现次数最多的词（一样多时留更常用的）")}>
                    <span>{tr("数量上限")}</span>
                    <select
                      value={customLimit ? "custom" : String(opts.limit)}
                      onChange={(e) => {
                        if (e.target.value === "custom") {
                          setCustomLimit(true);
                          if (LIMITS.includes(opts.limit)) set("limit", 400);
                        } else {
                          setCustomLimit(false);
                          set("limit", Number(e.target.value));
                        }
                      }}
                    >
                      {LIMITS.map((n) => (
                        <option key={n} value={n}>
                          {n === 0 ? tr("不限") : tr("最多 {n} 个", { n })}
                        </option>
                      ))}
                      <option value="custom">{tr("自定义…")}</option>
                    </select>
                  </label>
                  {customLimit && (
                    <label>
                      <span>{tr("最多几个")}</span>
                      <input
                        type="number"
                        min={1}
                        max={20000}
                        step={50}
                        value={opts.limit || ""}
                        onChange={(e) => set("limit", Math.max(0, Math.floor(Number(e.target.value) || 0)))}
                      />
                    </label>
                  )}
                </div>
                {opts.limit > 0 && available > opts.limit && (
                  <p className={styles.note}>{tr("这一档有 {all} 个，只留书里出现次数最多的 {n} 个。", { all: available.toLocaleString(), n: opts.limit.toLocaleString() })}</p>
                )}
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
                <div className={dlg.checks}>
                  <label>
                    <input type="checkbox" checked={opts.simple} onChange={(e) => set("simple", e.target.checked)} />
                    {tr("简明单词表（单词、词性、中文，像课本后面的单词表，快速过一遍）")}
                  </label>
                  <label>
                    <input type="checkbox" checked={opts.detail} onChange={(e) => set("detail", e.target.checked)} />
                    {tr("详细词表（书中原句、词典释义）")}
                  </label>
                </div>
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
                    <select value={opts.dict} onChange={(e) => set("dict", e.target.value)}>
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
                    <input type="checkbox" checked={opts.sentence} disabled={!opts.detail} onChange={(e) => set("sentence", e.target.checked)} />
                    {tr("书中原句")}
                  </label>
                  <label title={tr("各义项的中文，取自选的释义词典；它不是双解词典或没收这个词时，按配置组顺序找别的双解词典")}>
                    <input type="checkbox" checked={opts.gloss} onChange={(e) => set("gloss", e.target.checked)} />
                    {tr("中文释义（各义项）")}
                  </label>
                  <label>
                    <input type="checkbox" checked={opts.definition} disabled={!opts.detail} onChange={(e) => set("definition", e.target.checked)} />
                    {tr("词典释义")}
                  </label>
                  <label>
                    <input type="checkbox" checked={opts.examples} disabled={!opts.detail || !opts.definition} onChange={(e) => set("examples", e.target.checked)} />
                    {tr("词典例句")}
                  </label>
                  <label>
                    <input type="checkbox" checked={opts.phonetics} disabled={!opts.detail || !opts.definition} onChange={(e) => set("phonetics", e.target.checked)} />
                    {tr("音标")}
                  </label>
                </div>
                {opts.detail && opts.definition && count > LONG_LIST && (
                  <p className={styles.note}>{tr("{n} 个词带完整词典释义，PDF 会很长，生成要几分钟。只要中文简释可以取消「词典释义」。", { n: count.toLocaleString() })}</p>
                )}
              </fieldset>

              <fieldset>
                <legend>{tr("还包括")}</legend>
                <div className={dlg.checks}>
                  <label>
                    <input type="checkbox" checked={opts.guide} onChange={(e) => set("guide", e.target.checked)} />
                    {tr("读前导读（作者、背景、简介、主题、评价）")}
                  </label>
                </div>
                {opts.guide && (
                  <div className={dlg.selects}>
                    <label>
                      <span>{tr("导读来源")}</span>
                      <select value={opts.guideSource} onChange={(e) => set("guideSource", e.target.value as Opts["guideSource"])}>
                        <option value="wiki">{tr("维基百科（免费，联网）")}</option>
                        <option value="ai">{tr("AI 生成（按用量计费）")}</option>
                        <option value="both">{tr("两份都放")}</option>
                      </select>
                    </label>
                  </div>
                )}
                {opts.guide && opts.guideSource !== "ai" && <GuideStatus state={guide} spoilers={study?.spoilers ?? false} />}
                {opts.guide && opts.guideSource === "wiki" && guide.kind === "none" && (
                  <p className={styles.note}>{tr("可以把导读来源改成「AI 生成」。")}</p>
                )}
                {opts.guide && opts.guideSource !== "wiki" && (
                  <p className={styles.note} role="status">
                    {aiGuide.kind === "found"
                      ? tr("AI 导读已生成（{model}）", { model: aiGuide.guide.ai?.model ?? "" }) + (aiGuide.guide.ai?.known === false ? tr("，模型表示不了解这本书") : "")
                      : aiGuide.kind === "loading"
                        ? tr("AI 正在写导读，大约半分钟到一分钟…")
                        : aiGuide.kind === "error"
                          ? tr("AI 导读失败：{msg}", { msg: aiGuide.message })
                          : tr("AI 导读还没生成：会把书名、作者、章节标题和开头约两千字发给 {provider}，按用量计费，结果会存下来，同一本书不再收费。", {
                              provider: aiProvider === "deepseek" ? "DeepSeek" : "Claude",
                            })}{" "}
                    {aiGuide.kind !== "loading" && (
                      <button type="button" className={dlg.ghost} onClick={() => void makeAiGuide(aiGuide.kind === "found")}>
                        {aiGuide.kind === "found" ? tr("重新生成") : tr("用 AI 生成导读")}
                      </button>
                    )}
                  </p>
                )}
                <div className={dlg.checks}>
                  <label>
                    <input type="checkbox" checked={opts.roadmap} onChange={(e) => set("roadmap", e.target.checked)} />
                    {tr("阅读路线表（每章篇幅、预计用时、生词密度）")}
                  </label>
                  <label>
                    <input type="checkbox" checked={opts.index} disabled={opts.order !== "chapter" || !opts.detail} onChange={(e) => set("index", e.target.checked)} />
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
        <label className={dlg.name}>
          {tr("文件名")}
          <input value={name} onChange={(e) => setName(e.target.value)} spellCheck={false} />
          <span>.{format}</span>
        </label>
        {running && (
          <span className={dlg.status} role="status">
            {tr("已有一个预习包在后台生成，完成后再开始下一个")}
          </span>
        )}
        <button type="button" className={dlg.ghost} onClick={onClose}>
          {tr("取消")}
        </button>
        <button type="button" className={dlg.primary} disabled={!preview || running || count === 0} onClick={() => void generate()} title={tr("选好保存位置后在后台生成，可以关掉对话框继续做别的")}>
          {tr("在后台生成 {n} 个词的预习包", { n: count.toLocaleString() })}
        </button>
      </footer>
    </dialog>
  );
}

/** 读前导读找到了哪篇条目；剧透、语言在设置里改 */
function GuideStatus({ state, spoilers }: { state: GuideState; spoilers: boolean }) {
  const text =
    state.kind === "loading"
      ? tr("正在查维基百科…")
      : state.kind === "none"
        ? tr("维基百科上没找到这本书，导读这部分不放")
        : state.kind === "error"
          ? tr("查维基百科失败：{msg}", { msg: state.message })
          : tr("找到：{title}（{lang}）{author}", {
              title: state.guide.book.title,
              lang: state.guide.book.lang === "zh" ? tr("中文维基百科") : tr("英文维基百科"),
              author: state.guide.author ? tr("，作者：{name}", { name: state.guide.author.title }) : "",
            });
  return (
    <p className={styles.note} role="status">
      {text}
      {state.kind === "found" && ` · ${tr(spoilers ? "含情节梗概（会剧透）" : "不剧透")}`}
      {state.kind === "found" && (
        <>
          {" · "}
          <a
            href={state.guide.grokipedia}
            onClick={(e) => {
              e.preventDefault();
              void openUrl(state.guide.grokipedia);
            }}
          >
            {tr("在 Grokipedia 上看")}
          </a>
        </>
      )}
      {" · "}
      {tr("剧透和语言在 设置 › 全书预习包")}
    </p>
  );
}
