// 隐藏的导出窗口（SPEC 9.1 export-render）：渲染导出文档 → 等资源就绪 → 修正对比度 →
// 打印 PDF 或序列化为单文件 HTML → 回报结果。窗口不可见，所以不依赖 requestAnimationFrame。
//
// 两种任务：
// - export:render：单词导出（主窗口已组装好文档），结果发回主窗口
// - batch:render：批量导出（Rust 流水线发来原始词条），在这里清洗、组装，结果回报给 Rust

import { invoke } from "@tauri-apps/api/core";
import { emitTo } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { useEffect } from "react";
import { adjustContrast, parseColor } from "../../dict-render/contrast";
import { prepareEntry } from "../../dict-render/prepare";
import { inlineEntry } from "../../export/resources";
import type { ExportJob } from "../../export/run";
import exportCss from "../../export-templates/export.css?raw";
import { buildExportBody, buildVolumeBody, type ExportCard } from "../../export-templates/template";
import { isLang, setLang } from "../../i18n";
import type { RenderJob } from "../../ipc/batch";
import tokensCss from "../../theme/tokens.css?raw";
import { getSettings } from "../../ipc/settings";
import { applyTheme, customCss, findTheme, isAccentId, type CustomTheme } from "../../theme/themes";

/** 自定义主题要从设置里取颜色（导出窗口不常驻设置 store） */
async function lookupTheme(theme: string): Promise<{ dark: boolean; custom?: CustomTheme }> {
  if (!theme.startsWith("custom-")) return { dark: findTheme(theme)?.dark ?? false };
  const customs = (await getSettings()).customThemes ?? [];
  const info = findTheme(theme, customs);
  return { dark: info?.dark ?? false, custom: info?.custom };
}

const RESOURCE_TIMEOUT_MS = 8000;

function withTimeout<T>(p: Promise<T>, ms: number): Promise<T | void> {
  return Promise.race([p, new Promise<void>((r) => setTimeout(r, ms))]);
}

/** 等所有 Shadow DOM 里的样式表与图片加载完，再等字体。 */
async function waitResources(container: HTMLElement) {
  const roots = Array.from(container.querySelectorAll(".dict-body"))
    .map((h) => h.shadowRoot)
    .filter((r): r is ShadowRoot => r !== null);
  const waits: Promise<unknown>[] = [];
  for (const root of [container, ...roots] as ParentNode[]) {
    for (const link of Array.from(root.querySelectorAll("link"))) {
      if (!link.sheet) waits.push(new Promise((r) => (link.addEventListener("load", r), link.addEventListener("error", r))));
    }
    for (const img of Array.from(root.querySelectorAll("img"))) {
      if (!img.complete) waits.push(new Promise((r) => (img.addEventListener("load", r), img.addEventListener("error", r))));
    }
  }
  await withTimeout(Promise.all(waits), RESOURCE_TIMEOUT_MS);
  await withTimeout(document.fonts.ready, 2000);
  return roots;
}

/** 把文档放进页面并等它渲染就绪（含对比度修正）。 */
async function mount(body: string, theme: string, accent: string): Promise<{ container: HTMLElement; custom?: CustomTheme }> {
  const html = document.documentElement;
  const { dark, custom } = await lookupTheme(theme);
  applyTheme(custom ? "paper" : theme, isAccentId(accent) ? accent : "blue", html, custom);

  let style = document.getElementById("lens-export-css");
  if (!style) {
    style = document.createElement("style");
    style.id = "lens-export-css";
    document.head.append(style);
  }
  style.textContent = exportCss;

  let container = document.getElementById("lens-export-root");
  if (!container) {
    container = document.createElement("div");
    container.id = "lens-export-root";
    document.body.append(container);
  }
  // setHTMLUnsafe 会解析声明式 Shadow DOM（<template shadowrootmode>），innerHTML 不会
  (container as HTMLElement & { setHTMLUnsafe(h: string): void }).setHTMLUnsafe(body);

  const roots = await waitResources(container);
  const cs = getComputedStyle(html);
  const panel = parseColor(cs.getPropertyValue("--panel"))?.rgb;
  const raised = parseColor(cs.getPropertyValue("--raised"))?.rgb;
  if (panel && raised) {
    for (const r of roots) adjustContrast(r, { panel, raised, dark });
  }
  return { container, custom };
}

/** 序列化渲染结果（含对比度修正后的内联颜色）；主题变量与模板样式一并内嵌。 */
function serialize(container: HTMLElement, title: string, theme: string, accent: string, custom?: CustomTheme): string {
  const body = (container as HTMLElement & { getHTML(o: object): string }).getHTML({ serializableShadowRoots: true });
  const base = custom ? (custom.dark ? "dark" : "paper") : theme;
  return `<!doctype html>
<html lang="zh-CN" data-theme="${base}" data-accent="${accent}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${title.replace(/</g, "&lt;")}</title>
<style>${tokensCss}
${custom ? customCss(custom) : ""}
${exportCss}</style>
</head>
<body>${body}</body>
</html>
`;
}

/** 渲染完释放 DOM：批量导出时窗口一直复用，不留上一词的内容。 */
function unmount() {
  document.getElementById("lens-export-root")?.replaceChildren();
}

async function renderSingle(job: ExportJob) {
  setLang(job.lang ?? "zh");
  const { container, custom } = await mount(job.body, job.theme, job.accent);
  if (job.format === "pdf") {
    await invoke("export_print_pdf", { path: job.path, paper: job.paper });
  } else {
    await invoke("export_save_html", { path: job.path, html: serialize(container, job.title, job.theme, job.accent, custom) });
  }
}

/** 批量导出：原始词条 → 清洗（与查词页同一套）→（HTML 需要时）内联资源 → 组装文档 → 打印 / 序列化。 */
async function renderBatch(job: RenderJob): Promise<string | null> {
  setLang(isLang(job.lang) ? job.lang : "zh");
  const words = await Promise.all(
    job.words.map(async (w) => ({
      word: w.word,
      context: w.context,
      cards: await Promise.all(
        w.cards.map(
          async (c): Promise<ExportCard> => ({
            id: c.id,
            name: c.name,
            online: c.online,
            entries: await Promise.all(
              c.entries.map(async (e) => {
                const html = prepareEntry(e.body, c.id).html;
                // HTML 要单文件可用；内联后的文档打印 PDF 也一样
                return job.wantHtml ? inlineEntry(html) : html;
              }),
            ),
          }),
        ),
      ),
    })),
  );
  const common = { date: new Date(), layout: job.layout, style: job.style, options: job.options };
  const body =
    job.mode === "volume"
      ? buildVolumeBody({ ...common, title: job.title, words })
      : buildExportBody({ ...common, word: words[0]?.word ?? job.title, cards: words[0]?.cards ?? [], context: words[0]?.context ?? null });
  const theme = job.style === "inkSaving" ? "eink" : job.style === "followTheme" ? job.theme : "paper";
  const { container, custom } = await mount(body, theme, job.accent);
  if (job.pdfPath) {
    await invoke(job.mode === "volume" ? "export_print_pdf_outline" : "export_print_pdf", { path: job.pdfPath, paper: job.paper });
  }
  return job.wantHtml ? serialize(container, `${job.title} · Lens 词镜`, theme, job.accent, custom) : null;
}

export function ExportRenderWindow() {
  useEffect(() => {
    const win = getCurrentWebviewWindow();
    const single = win.listen<ExportJob>("export:render", async (e) => {
      const job = e.payload;
      try {
        await renderSingle(job);
        await emitTo("main", "export:done", { jobId: job.jobId, ok: true, error: null });
      } catch (err) {
        await emitTo("main", "export:done", { jobId: job.jobId, ok: false, error: String(err) });
      }
    });
    const batch = win.listen<RenderJob>("batch:render", async (e) => {
      const job = e.payload;
      try {
        const html = await renderBatch(job);
        unmount();
        await invoke("batch_render_done", { jobId: job.jobId, ok: true, error: null, html });
      } catch (err) {
        unmount();
        await invoke("batch_render_done", { jobId: job.jobId, ok: false, error: String(err), html: null });
      }
    });
    // 告诉主窗口与批量导出流水线：已就绪，可以接任务
    void Promise.all([single, batch]).then(() => {
      void emitTo("main", "export:ready", null);
      void invoke("batch_window_ready");
    });
    return () => {
      void single.then((off) => off());
      void batch.then((off) => off());
    };
  }, []);
  return null;
}
