// 全书预习包的后台生成：对话框选好选项和保存位置就关掉，这里接着取释义、查词典、排版、打印 PDF，
// 右下角的进度条（StudyJobChip）显示进度，可以停止；用户这期间可以照常查词、阅读。
// 同一时间只跑一个；退出 Lens 会中断（下次重新生成）。

import { create } from "zustand";
import { prepareEntry } from "../dict-render/prepare";
import { buildStudyBody, type StudyExportWord } from "../export-templates/study";
import type { ExportStyle, Paper } from "../export-templates/template";
import { inlineEntry } from "../export/resources";
import { runExport } from "../export/run";
import type { Lang } from "../i18n";
import { tr } from "../i18n";
import { firstEntry, senseGlosses, type SenseGloss } from "../ipc/lookup";
import { saveScannedWords } from "../ipc/scan";
import { bookStudyList, type BookGuide, type Scope, type StudyOrder } from "../ipc/study";

export interface StudyJobSpec {
  bookId: number;
  title: string;
  author: string | null;
  level: number;
  minCount: number;
  includeSaved: boolean;
  scope: Scope;
  order: StudyOrder;
  limit: number | null;
  /** 释义先查的词典 id；null = 自动 */
  dict: string | null;
  gloss: boolean;
  sentence: boolean;
  definition: boolean;
  examples: boolean;
  phonetics: boolean;
  roadmap: boolean;
  index: boolean;
  simple: boolean;
  detail: boolean;
  save: boolean;
  guides: BookGuide[];
  stats: { total: number; minutes: number; coverage: number };
  format: "pdf" | "html";
  style: ExportStyle;
  theme: string;
  accent: string;
  paper: Paper;
  path: string;
  lang: Lang;
}

export interface StudyJob {
  title: string;
  state: "running" | "done" | "error" | "stopped";
  /** 正在做什么（取简释、查词典、排版…） */
  note: string;
  /** 进度 0–1；不知道时为 null */
  progress: number | null;
  path: string;
  words: number;
  message: string | null;
}

const GLOSS_BATCH = 100;
/** 上千个词的 PDF 排版、打印要几分钟 */
const EXPORT_TIMEOUT_MS = 15 * 60_000;

let cancelled = false;

interface StudyJobState {
  job: StudyJob | null;
  /** 开始后台生成（已有一个在跑时不开始，返回 false） */
  start: (spec: StudyJobSpec) => boolean;
  stop: () => void;
  /** 关掉完成 / 失败的提示 */
  dismiss: () => void;
}

export const useStudyJob = create<StudyJobState>((set, get) => {
  const progress = (note: string, p: number | null) =>
    set((s) => (s.job && s.job.state === "running" ? { job: { ...s.job, note, progress: p } } : s));

  const run = async (spec: StudyJobSpec) => {
    const stop = () => {
      if (cancelled) throw new Error(tr("已停止"));
    };
    progress(tr("正在挑选生词…"), null);
    const list = await bookStudyList(spec.bookId, spec.level, spec.minCount, spec.includeSaved, spec.scope, spec.order, spec.limit);
    if (list.words.length === 0) throw new Error(tr("这个门槛下没有生词，换一档词汇量试试"));
    const n = list.words.length;

    // 词性和各义项的中文：先查选的释义词典，再按配置组顺序查别的双解词典；原形查不到时查书里的写法
    const glosses = new Map<string, SenseGloss>();
    for (let i = 0; i < n; i += GLOSS_BATCH) {
      stop();
      progress(tr("正在取简释 {i} / {n}", { i: Math.min(i + GLOSS_BATCH, n), n }), (0.15 * i) / n);
      const batch = await senseGlosses(
        list.words.slice(i, i + GLOSS_BATCH).map((w) => ({ lemma: w.lemma, forms: w.forms })),
        spec.dict,
      ).catch(() => []);
      for (const g of batch) glosses.set(g.word, g);
    }

    const withEntries = spec.detail && spec.definition;
    const rows: StudyExportWord[] = [];
    for (const [i, w] of list.words.entries()) {
      stop();
      let entry: StudyExportWord["entry"] = null;
      if (withEntries) {
        if (i % 10 === 0) progress(tr("正在查词典 {i} / {n}", { i: i + 1, n }), 0.15 + (0.6 * i) / n);
        const r = await firstEntry(w.lemma, spec.dict, w.forms).catch(() => null);
        if (r) {
          const html = r.entries.map((e) => prepareEntry(e.body, r.dictId).html);
          entry = { name: r.name, html: spec.format === "html" ? await Promise.all(html.map(inlineEntry)) : html };
        }
      }
      const g = glosses.get(w.lemma);
      rows.push({
        lemma: w.lemma,
        surface: w.sentence ? w.sentence.slice(w.hlStart, w.hlEnd) : (w.forms[0] ?? w.lemma),
        status: w.status,
        pos: g?.pos ?? null,
        gloss: spec.gloss ? (g?.senses[0] ?? null) : null,
        senses: spec.gloss ? g?.senses : undefined,
        sentence: w.sentence,
        hlStart: w.hlStart,
        hlEnd: w.hlEnd,
        entry,
        count: w.count,
        chapter: w.chapter,
      });
    }

    if (spec.save) {
      stop();
      progress(tr("正在收进生词本…"), 0.78);
      const items = rows
        .filter((r) => r.status === "new")
        .map((r) => ({ lemma: r.lemma, surface: r.surface, sentence: r.sentence, hlStart: r.hlStart, hlEnd: r.hlEnd, brief: r.gloss }));
      await saveScannedWords(items, spec.title, [spec.title]);
    }

    stop();
    progress(tr("正在排版，词多时要几分钟…"), 0.8);
    const body = buildStudyBody({
      title: spec.title,
      author: spec.author,
      words: rows,
      chapters: list.chapters,
      order: spec.order,
      style: spec.style,
      date: new Date(),
      level: spec.level,
      options: {
        sentence: spec.sentence,
        definition: spec.definition,
        examples: spec.examples,
        phonetics: spec.phonetics,
        roadmap: spec.roadmap,
        index: spec.index,
        simple: spec.simple,
        detail: spec.detail,
      },
      stats: spec.stats,
      guides: spec.guides,
    });
    await runExport({
      jobId: crypto.randomUUID(),
      format: spec.format,
      body,
      title: `${spec.title} · ${tr("读前预习")}`,
      theme: spec.theme,
      accent: spec.accent,
      path: spec.path,
      paper: spec.paper,
      lang: spec.lang,
      outline: spec.format === "pdf",
      timeoutMs: EXPORT_TIMEOUT_MS,
    });
    return rows.length;
  };

  return {
    job: null,
    start: (spec) => {
      if (get().job?.state === "running") return false;
      cancelled = false;
      set({ job: { title: spec.title, state: "running", note: tr("准备中…"), progress: 0, path: spec.path, words: 0, message: null } });
      run(spec).then(
        (words) => set((s) => ({ job: s.job && { ...s.job, state: "done", words, progress: 1, note: "" } })),
        (e) =>
          set((s) => ({
            job: s.job && { ...s.job, state: cancelled ? "stopped" : "error", message: String(e instanceof Error ? e.message : e) },
          })),
      );
      return true;
    },
    stop: () => {
      cancelled = true;
    },
    dismiss: () => {
      if (get().job?.state !== "running") set({ job: null });
    },
  };
});
