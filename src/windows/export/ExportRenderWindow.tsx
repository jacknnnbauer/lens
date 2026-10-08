// 隐藏的导出窗口（SPEC 9.1 export-render）：渲染导出文档 → 等资源就绪 → 修正对比度 →
// 打印 PDF 或序列化为单文件 HTML → 把结果告诉主窗口。窗口不可见，所以不依赖 requestAnimationFrame。

import { invoke } from "@tauri-apps/api/core";
import { emitTo } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { useEffect } from "react";
import { adjustContrast, parseColor } from "../../dict-render/contrast";
import type { ExportJob } from "../../export/run";
import exportCss from "../../export-templates/export.css?raw";
import tokensCss from "../../theme/tokens.css?raw";
import { THEMES } from "../../theme/themes";

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

async function render(job: ExportJob) {
  const html = document.documentElement;
  html.dataset.theme = job.theme;
  html.dataset.accent = job.accent;

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
  (container as HTMLElement & { setHTMLUnsafe(h: string): void }).setHTMLUnsafe(job.body);

  const roots = await waitResources(container);
  const cs = getComputedStyle(html);
  const panel = parseColor(cs.getPropertyValue("--panel"))?.rgb;
  const raised = parseColor(cs.getPropertyValue("--raised"))?.rgb;
  if (panel && raised) {
    const dark = THEMES.find((t) => t.id === job.theme)?.dark ?? false;
    for (const r of roots) adjustContrast(r, { panel, raised, dark });
  }

  if (job.format === "pdf") {
    await invoke("export_print_pdf", { path: job.path, paper: job.paper });
  } else {
    // 序列化渲染结果（含对比度修正后的内联颜色）；主题变量与模板样式一并内嵌
    const body = (container as HTMLElement & { getHTML(o: object): string }).getHTML({ serializableShadowRoots: true });
    const doc = `<!doctype html>
<html lang="zh-CN" data-theme="${job.theme}" data-accent="${job.accent}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${job.title.replace(/</g, "&lt;")}</title>
<style>${tokensCss}
${exportCss}</style>
</head>
<body>${body}</body>
</html>
`;
    await invoke("export_save_html", { path: job.path, html: doc });
  }
}

export function ExportRenderWindow() {
  useEffect(() => {
    const win = getCurrentWebviewWindow();
    const sub = win.listen<ExportJob>("export:render", async (e) => {
      const job = e.payload;
      try {
        await render(job);
        await emitTo("main", "export:done", { jobId: job.jobId, ok: true, error: null });
      } catch (err) {
        await emitTo("main", "export:done", { jobId: job.jobId, ok: false, error: String(err) });
      }
    });
    // 告诉主窗口：已就绪，可以接任务（首次创建窗口时主窗口在等这个）
    void sub.then(() => emitTo("main", "export:ready", null));
    return () => void sub.then((off) => off());
  }, []);
  return null;
}
