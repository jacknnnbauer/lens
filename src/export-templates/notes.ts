// 读书笔记导出（阅读 › 批注 › 导出 PDF）：封面（书名、作者、条数、时间跨度）+ 按章节排的标注卡片。
// 每条：左边一道标注色，衬线字的原文，下面是笔记和标注时间。页眉书名、页脚页码用 @page 边框区。

import { escapeHtml } from "../copy/convert";
import { tr } from "../i18n";
import { isoDate, isoMinute } from "./template";

export interface NotesItem {
  quote: string;
  note: string | null;
  color: string;
  createdAt: number;
  /** 正文改过、原文找不到了 */
  missing?: boolean;
}

export interface NotesInput {
  title: string;
  author: string | null;
  /** 普通文章只有一组、标题为空 */
  chapters: { title: string; items: NotesItem[] }[];
  exportedAt: Date;
}

// 打印用的柔和色条
const BAR: Record<string, string> = { yellow: "#e8b931", green: "#4fb37f", blue: "#3fa7d6", pink: "#e27aa0" };

const esc = (s: string) => escapeHtml(s);
const lines = (s: string) => esc(s).replace(/\r?\n/g, "<br>");

function css(title: string): string {
  // @page 的 content 是 CSS 字符串：引号和反斜杠要转义
  const head = title.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
  return `
@page { margin: 22mm 20mm 20mm;
  @top-center { content: "${head}"; font: 9pt var(--font-ui); color: #999; }
  @bottom-center { content: counter(page); font: 9pt var(--font-ui); color: #999; } }
@page :first { @top-center { content: none; } @bottom-center { content: none; } }
.nt { font-family: var(--font-ui); color: var(--text); }
.nt-cover { min-height: 60vh; display: flex; flex-direction: column; justify-content: center; padding: 0 4px; break-after: page; }
.nt-kicker { margin: 0 0 18px; font-size: 12px; letter-spacing: .3em; color: var(--faint); }
.nt-title { margin: 0; font: 600 34px/1.25 var(--font-gloss); }
.nt-author { margin: 10px 0 0; font: italic 17px var(--font-gloss); color: var(--sub); }
.nt-rule { width: 56px; height: 2px; margin: 28px 0 16px; background: var(--accent); }
.nt-stats { margin: 0; font-size: 13px; color: var(--sub); line-height: 1.8; }
.nt-chapter { margin: 28px 0 14px; padding-bottom: 6px; border-bottom: 1px solid var(--line); font: 600 16px var(--font-gloss); break-after: avoid; }
.nt-item { margin: 0 0 16px; padding: 2px 0 2px 16px; border-left: 3px solid var(--bar); break-inside: avoid; }
.nt-quote { margin: 0; font: 15px/1.7 var(--font-gloss); }
.nt-note { margin: 8px 0 0; padding: 8px 12px; border-radius: 6px; background: color-mix(in srgb, var(--bar) 10%, transparent); font-size: 13px; line-height: 1.65; }
.nt-note::before { content: "✎ "; color: var(--bar); }
.nt-meta { margin: 6px 0 0; font-size: 11px; color: var(--faint); font-variant-numeric: tabular-nums; }
.nt-lost { color: #c0392b; }
`;
}

export function buildNotesBody(input: NotesInput): string {
  const all = input.chapters.flatMap((c) => c.items);
  const notes = all.filter((i) => i.note?.trim()).length;
  const times = all.map((i) => i.createdAt).sort((a, b) => a - b);
  const span = times.length ? (isoDate(new Date(times[0])) === isoDate(new Date(times[times.length - 1])) ? isoDate(new Date(times[0])) : `${isoDate(new Date(times[0]))} — ${isoDate(new Date(times[times.length - 1]))}`) : "";

  const cover = `<section class="nt-cover">
<p class="nt-kicker">${esc(tr("读书笔记"))}</p>
<h1 class="nt-title">${esc(input.title)}</h1>
${input.author ? `<p class="nt-author">${esc(input.author)}</p>` : ""}
<div class="nt-rule"></div>
<p class="nt-stats">${esc(tr("{n} 条标注 · {m} 条笔记", { n: all.length, m: notes }))}<br>${esc(span)}<br>${esc(tr("导出于 {d}", { d: isoDate(input.exportedAt) }))}</p>
</section>`;

  const item = (i: NotesItem) => `<div class="nt-item" data-color="${esc(i.color)}" style="--bar:${BAR[i.color] ?? BAR.yellow}">
<p class="nt-quote">${lines(i.quote)}</p>
${i.note?.trim() ? `<p class="nt-note">${lines(i.note.trim())}</p>` : ""}
<p class="nt-meta">${esc(isoMinute(i.createdAt))}${i.missing ? ` · <span class="nt-lost">${esc(tr("原文已改动"))}</span>` : ""}</p>
</div>`;

  const body = input.chapters
    .filter((c) => c.items.length)
    .map((c) => `${c.title.trim() ? `<h2 class="nt-chapter">${esc(c.title)}</h2>` : ""}\n${c.items.map(item).join("\n")}`)
    .join("\n");

  return `<style>${css(input.title)}</style><article class="lens-export nt">${cover}\n${body}</article>`;
}
