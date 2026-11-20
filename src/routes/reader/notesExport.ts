// 批注列表的「导出 PDF」：选好保存位置就直接排版导出（读书笔记固定用白纸配色，打印也清楚）。

import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { buildNotesBody, type NotesInput } from "../../export-templates/notes";
import { exportTheme, isoDate, safeFileName } from "../../export-templates/template";
import { runExport } from "../../export/run";
import { getLang, tr } from "../../i18n";
import type { Annotation } from "../../ipc/annotations";
import { useSettings } from "../../stores/settings";
import { toast } from "../../stores/toast";
import { useAppliedTheme } from "../../theme/useApplyTheme";

export interface NotesSource {
  title: string;
  author: string | null;
  /** 电子书的章节名；普通文章为 null */
  chapterTitles: string[] | null;
  items: Annotation[];
  missing: Set<number>;
}

/** 按章节分组（章节顺序、组内按时间） */
export function notesInput(src: NotesSource, exportedAt: Date): NotesInput {
  const groups = new Map<number, Annotation[]>();
  for (const a of [...src.items].sort((x, y) => x.chapter - y.chapter || x.createdAt - y.createdAt)) groups.set(a.chapter, [...(groups.get(a.chapter) ?? []), a]);
  return {
    title: src.title,
    author: src.author,
    exportedAt,
    chapters: [...groups.entries()].map(([ch, list]) => ({
      title: src.chapterTitles ? (src.chapterTitles[ch] ?? tr("第 {n} 节", { n: ch + 1 })) : "",
      items: list.map((a) => ({ quote: a.quote, note: a.note, color: a.color, createdAt: a.createdAt, missing: src.missing.has(a.id) })),
    })),
  };
}

export async function exportNotesPdf(src: NotesSource): Promise<void> {
  if (src.items.length === 0) return;
  const s = useSettings.getState().settings;
  const dir = s?.export?.dir;
  const fileName = `${safeFileName(src.title) || "lens"}_${tr("读书笔记")}_${isoDate(new Date())}.pdf`;
  const path = await saveDialog({ title: tr("导出读书笔记"), defaultPath: dir ? `${dir}\\${fileName}` : fileName, filters: [{ name: "PDF", extensions: ["pdf"] }] });
  if (!path) return;
  toast(tr("正在导出读书笔记…"));
  try {
    await runExport({
      jobId: crypto.randomUUID(),
      format: "pdf",
      body: buildNotesBody(notesInput(src, new Date())),
      title: `${src.title} · ${tr("读书笔记")}`,
      theme: exportTheme("paperWhite", useAppliedTheme.getState().theme ?? "paper"),
      accent: s?.appearance.accent ?? "blue",
      path,
      paper: s?.export?.paper ?? "a4",
      lang: getLang(),
    });
    toast(tr("读书笔记已导出"), null, { label: tr("打开文件夹"), run: () => void revealItemInDir(path) });
  } catch (e) {
    toast(tr("导出失败：{e}", { e: String(e instanceof Error ? e.message : e) }));
  }
}
