// 全书预习包（阅读 › 导出 › 全书预习包）：卷首概况 → 阅读路线表（每章篇幅、用时、生词密度）→ 生词表 → 字母索引。
// 生词表按章节先后分章（每章背这一章第一次出现的词），或按字母分组；每个词的样式和文章导出的词汇表相同。
// 章名、字母都是 h2，打印 PDF 时生成书签目录。

import { escapeHtml } from "../copy/convert";
import { tr } from "../i18n";
import type { ChapterRow, StudyOrder } from "../ipc/study";
import { glossItem, type ArticleWord, type GlossOptions } from "./article";
import { COPYRIGHT, isoDate, type ExportStyle } from "./template";

export interface StudyExportWord extends ArticleWord {
  /** 全书出现次数、第一次出现的章 */
  count: number;
  chapter: number;
}

export interface StudyExportInput {
  title: string;
  author: string | null;
  words: StudyExportWord[];
  chapters: ChapterRow[];
  order: StudyOrder;
  style: ExportStyle;
  date: Date;
  level: number;
  options: GlossOptions & { roadmap: boolean; index: boolean };
  stats: { total: number; minutes: number; coverage: number };
}

/** 分钟数 → 「3 小时 20 分」/「45 分钟」 */
export function duration(minutes: number): string {
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  if (h === 0) return tr("{m} 分钟", { m });
  return m === 0 ? tr("{h} 小时", { h }) : tr("{h} 小时 {m} 分", { h, m });
}

/** 书里的章节序号 → 路线表里的第几行（只数算进来的章；书自己的章号在标题里） */
function chapterNumbers(chapters: ChapterRow[]): Map<number, number> {
  return new Map(chapters.map((c, i) => [c.idx, i + 1]));
}

function roadmap(input: StudyExportInput, no: Map<number, number>): string {
  const max = Math.max(1, ...input.chapters.map((c) => c.density));
  const rows = input.chapters
    .map((c) => {
      const pct = Math.round((c.density / max) * 100);
      return `<tr><td class="st-n">${no.get(c.idx) ?? ""}</td><td class="st-title">${escapeHtml(c.title)}</td><td class="st-num">${c.tokens.toLocaleString()}</td><td class="st-num">${duration(c.minutes)}</td><td class="st-num">${c.firstWords}</td><td class="st-density"><span class="st-bar" style="width:${pct}%"></span><span class="st-num">${c.density.toFixed(1)}</span></td></tr>`;
    })
    .join("");
  return `<section class="st-road"><h2>${tr("阅读路线表")}<small aria-hidden="true">${tr("{n} 部分", { n: input.chapters.length })}</small></h2>
<p class="st-note">${tr("预计用时按每分钟 180 词估算；新生词是这一章第一次出现的生词；生词密度是每千词里生词出现的次数，越高越难读。")}</p>
<table class="st-table"><thead><tr><th>#</th><th>${tr("标题")}</th><th>${tr("词数")}</th><th>${tr("预计用时")}</th><th>${tr("新生词")}</th><th>${tr("生词密度")}</th></tr></thead><tbody>${rows}</tbody></table></section>`;
}

function extra(w: StudyExportWord, input: StudyExportInput, chapterTitle: Map<number, string>): string {
  const times = `<span class="gl-form">${tr("全书 {n} 次", { n: w.count })}</span>`;
  if (input.order === "chapter") return times;
  const where = chapterTitle.get(w.chapter);
  return where ? `${times}<span class="gl-form">${tr("首见于 {title}", { title: escapeHtml(where) })}</span>` : times;
}

function vocabulary(input: StudyExportInput): string {
  const titles = new Map(input.chapters.map((c) => [c.idx, c.title]));
  const numbered = input.words.map((w, i) => ({ w, n: i + 1 }));
  const item = ({ w, n }: { w: StudyExportWord; n: number }) => glossItem(w, n, input.options, input.style, extra(w, input, titles));
  const groups: Array<{ head: string; items: typeof numbered }> = [];
  if (input.order === "chapter") {
    // 用书自己的章节标题：小说正文前常有 Etymology 这类部分，自己编号会和书里的章号对不上
    for (const c of input.chapters) {
      const items = numbered.filter(({ w }) => w.chapter === c.idx);
      if (items.length) groups.push({ head: escapeHtml(c.title), items });
    }
  } else {
    for (const x of numbered) {
      const letter = (x.w.lemma[0] ?? "#").toUpperCase();
      const last = groups[groups.length - 1];
      if (last && last.head === letter) last.items.push(x);
      else groups.push({ head: letter, items: [x] });
    }
  }
  return groups
    .map((g, i) => `<section class="glossary st-group"${i > 0 ? ' data-follow=""' : ""}><h2>${g.head}<small aria-hidden="true">${tr("{n} 个词", { n: g.items.length })}</small></h2><div class="gl-list">${g.items.map(item).join("")}</div></section>`)
    .join("");
}

/** 按章排的词表后面附一份字母索引：词 → 编号 */
function alphaIndex(input: StudyExportInput): string {
  const entries = input.words.map((w, i) => ({ lemma: w.lemma, n: i + 1 })).sort((a, b) => a.lemma.localeCompare(b.lemma));
  const items = entries.map((e) => `<li><span>${escapeHtml(e.lemma)}</span><span class="st-num">${e.n}</span></li>`).join("");
  return `<section class="st-index"><h2>${tr("字母索引")}<small aria-hidden="true">${tr("词 → 编号")}</small></h2><ul>${items}</ul></section>`;
}

/** 全书预习包的 <body> 内容 */
export function buildStudyBody(input: StudyExportInput): string {
  const no = chapterNumbers(input.chapters);
  const meta = [
    input.author ? escapeHtml(input.author) : "",
    tr("{n} 部分", { n: input.chapters.length }),
    tr("{n} 词", { n: input.stats.total.toLocaleString() }),
    tr("预计阅读 {t}", { t: duration(input.stats.minutes) }),
    tr("常用 {level} 词以外的生词 {n} 个", { level: input.level.toLocaleString(), n: input.words.length }),
    tr("认识 {pct}%", { pct: (input.stats.coverage * 100).toFixed(1) }),
    `${tr("Lens 词镜")} · ${isoDate(input.date)}`,
  ]
    .filter(Boolean)
    .join(" · ");
  const style = input.style === "inkSaving" ? "ink" : input.style.startsWith("theme:") ? "theme" : input.style;
  return `<main class="lens-export lens-article lens-study" data-layout="book" data-style="${style}"><header class="ar-head"><div class="ar-kicker">${tr("读前预习")}</div><h1>${escapeHtml(input.title)}</h1><p class="ar-meta">${meta}</p></header>${input.options.roadmap ? roadmap(input, no) : ""}${vocabulary(input)}${input.options.index && input.order === "chapter" && input.words.length > 0 ? alphaIndex(input) : ""}<footer class="copyright">${tr(COPYRIGHT)}</footer></main>`;
}
