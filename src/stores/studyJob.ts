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
import { bookDigest, bookDigestPlan, bookDigestStep, type Digest } from "../ipc/digest";
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
  /** 先让 AI 通读全书写精读本（放在最前面） */
  digest: boolean;
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

/** 同时跑几步通读（不同的段互不相关） */
const DIGEST_PARALLEL = 4;

/**
 * 精读本：先按段通读（并发），再做四步综合。每步后端都存了文件，做过的直接跳过；
 * 某一步失败重试一次，还不行就报错（已做完的保留，下次接着做）。
 */
async function makeDigest(bookId: number, report: (note: string, p: number | null) => void, stop: () => void): Promise<Digest> {
  report(tr("精读本：准备中…"), null);
  const plan = await bookDigestPlan(bookId);
  if (!plan.hasKey && plan.done.length < plan.steps) throw new Error(tr("精读本要用 AI：先在设置 › AI 例句里填写 {provider} API Key", { provider: plan.provider }));
  const done = new Set(plan.done);
  const total = plan.steps;
  const tick = (note: string) => report(note, done.size / total);
  // 网络偶尔断（长请求要几分钟）：失败后等 5 秒、20 秒再试，共三次
  const once = async (step: number) => {
    for (let attempt = 0; ; attempt++) {
      stop();
      try {
        await bookDigestStep(bookId, step);
        break;
      } catch (e) {
        if (attempt >= 2) throw e;
        await new Promise((r) => setTimeout(r, attempt === 0 ? 5_000 : 20_000));
      }
    }
    done.add(step);
  };
  const reading = Array.from({ length: plan.chunks }, (_, i) => i).filter((i) => !done.has(i));
  const readNote = () => tr("精读本：AI 正在通读全书 {i} / {n} 段", { i: [...done].filter((s) => s < plan.chunks).length, n: plan.chunks });
  tick(readNote());
  let next = 0;
  const worker = async () => {
    while (next < reading.length) {
      const step = reading[next++];
      await once(step);
      tick(readNote());
    }
  };
  await Promise.all(Array.from({ length: Math.min(DIGEST_PARALLEL, reading.length) }, worker));
  // 综合的四步互不依赖，一起做
  const synth = Array.from({ length: total - plan.chunks }, (_, i) => plan.chunks + i).filter((s) => !done.has(s));
  if (synth.length) {
    tick(tr("精读本：正在写人物、主题、问答和自测（几分钟）…"));
    await Promise.all(synth.map(once));
  }
  stop();
  const d = await bookDigest(bookId);
  if (!d) throw new Error(tr("精读本没有做完，请重试"));
  return d;
}

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
    // 精读本占进度条的前一半
    let digest: Digest | null = null;
    let base = 0;
    let span = 1;
    if (spec.digest) {
      digest = await makeDigest(spec.bookId, (note, p) => progress(note, p == null ? null : p * 0.5), stop);
      base = 0.5;
      span = 0.5;
    }
    const report = (note: string, p: number | null) => progress(note, p == null ? null : base + p * span);
    report(tr("正在挑选生词…"), null);
    const list = await bookStudyList(spec.bookId, spec.level, spec.minCount, spec.includeSaved, spec.scope, spec.order, spec.limit);
    if (list.words.length === 0) throw new Error(tr("这个门槛下没有生词，换一档词汇量试试"));
    const n = list.words.length;

    // 词性和各义项的中文：先查选的释义词典，再按配置组顺序查别的双解词典；原形查不到时查书里的写法
    const glosses = new Map<string, SenseGloss>();
    for (let i = 0; i < n; i += GLOSS_BATCH) {
      stop();
      report(tr("正在取简释 {i} / {n}", { i: Math.min(i + GLOSS_BATCH, n), n }), (0.15 * i) / n);
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
        if (i % 10 === 0) report(tr("正在查词典 {i} / {n}", { i: i + 1, n }), 0.15 + (0.6 * i) / n);
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
      report(tr("正在收进生词本…"), 0.78);
      const items = rows
        .filter((r) => r.status === "new")
        .map((r) => ({ lemma: r.lemma, surface: r.surface, sentence: r.sentence, hlStart: r.hlStart, hlEnd: r.hlEnd, brief: r.gloss }));
      await saveScannedWords(items, spec.title, [spec.title]);
    }

    stop();
    report(tr("正在排版，词多时要几分钟…"), 0.8);
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
      digest,
    });
    await runExport({
      jobId: crypto.randomUUID(),
      format: spec.format,
      body,
      title: `${spec.title} · ${digest ? tr("精读本") : tr("读前预习")}`,
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
