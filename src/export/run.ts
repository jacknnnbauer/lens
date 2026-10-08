// 主窗口侧的导出调度：确保导出窗口存在 → 等它就绪 → 发任务 → 等结果。

import { invoke } from "@tauri-apps/api/core";
import { emitTo } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { Paper } from "../export-templates/template";

export interface ExportJob {
  jobId: string;
  format: "pdf" | "html";
  /** 导出文档 <body> 内容 */
  body: string;
  title: string;
  theme: string;
  accent: string;
  path: string;
  paper: Paper;
}

const READY_TIMEOUT_MS = 20_000;
const DONE_TIMEOUT_MS = 180_000;

function waitFor<T>(event: string, ms: number, accept: (p: T) => boolean = () => true): { promise: Promise<T>; cancel: () => void } {
  let off: (() => void) | undefined;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const promise = new Promise<T>((resolve, reject) => {
    timer = setTimeout(() => reject(new Error(`等待 ${event} 超时`)), ms);
    void getCurrentWebviewWindow()
      .listen<T>(event, (e) => {
        if (!accept(e.payload)) return;
        clearTimeout(timer);
        off?.();
        resolve(e.payload);
      })
      .then((f) => (off = f));
  });
  return {
    promise,
    cancel: () => {
      clearTimeout(timer);
      off?.();
    },
  };
}

export async function runExport(job: ExportJob): Promise<void> {
  // 先挂上监听，再创建窗口，避免错过就绪事件
  const ready = waitFor<null>("export:ready", READY_TIMEOUT_MS);
  const created = await invoke<boolean>("export_prepare");
  if (created) await ready.promise;
  else ready.cancel();

  const done = waitFor<{ jobId: string; ok: boolean; error: string | null }>("export:done", DONE_TIMEOUT_MS, (p) => p.jobId === job.jobId);
  await emitTo("export", "export:render", job);
  const r = await done.promise;
  if (!r.ok) throw new Error(r.error ?? "导出失败");
}
