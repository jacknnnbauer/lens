// 全书预习包（阅读 › 导出 › 全书预习包）：卷首概况 → 读前导读（维基百科）→ 阅读路线表（每章篇幅、用时、生词密度）
// → 简明单词表（单词、词性、中文，像课本单词表）→ 详细词表（原句、词典释义）→ 字母索引。
// 生词按章节先后分组（每章是这一章第一次出现的词），或按字母分组；详细词表每个词的样式和文章导出的词汇表相同。
// 各部分标题是 h2、分组标题是 h3，打印 PDF 时生成两级书签。

import { escapeHtml } from "../copy/convert";
import { tr } from "../i18n";
import type { BookGuide, ChapterRow, GuideSection, StudyOrder } from "../ipc/study";
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
  /** simple：简明单词表；detail：详细词表（原句、词典释义） */
  options: GlossOptions & { roadmap: boolean; index: boolean; simple: boolean; detail: boolean };
  stats: { total: number; minutes: number; coverage: number };
  /** 读前导读（维基百科、AI，可以两份都有）；没选或没找到时为空 */
  guides: BookGuide[];
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

const KIND_LABEL: Record<GuideSection["kind"], string> = {
  lead: "简介",
  background: "创作背景",
  characters: "主要人物",
  plot: "情节梗概（含剧透）",
  themes: "主题与风格",
  reception: "评价与影响",
  other: "",
};

function paragraphs(text: string): string {
  return text
    .split("\n")
    .map((p) => p.trim())
    .filter(Boolean)
    .map((p) => `<p>${escapeHtml(p)}</p>`)
    .join("");
}

/** 读前导读：书的条目分节，作者条目的开头；维基百科的注明出处和许可，AI 的注明是 AI 写的 */
function guideSection(g: BookGuide): string {
  const lang = (l: string) => (l === "zh" ? tr("中文维基百科") : tr("英文维基百科"));
  const parts = g.book.sections.map((s) => {
    const label = tr(KIND_LABEL[s.kind]);
    // 同一类有几节（Structure、Themes、Style）时带上原标题
    const head = s.kind === "lead" ? label : label ? `${label} · ${escapeHtml(s.heading)}` : escapeHtml(s.heading);
    return `<h3>${head}</h3>${paragraphs(s.text)}`;
  });
  if (g.author) parts.push(`<h3>${tr("作者 · {name}", { name: escapeHtml(g.author.title) })}</h3>${paragraphs(g.author.sections[0]?.text ?? "")}`);
  const sources = [g.book, ...(g.author ? [g.author] : [])]
    .map((a) => `<a href="${escapeHtml(a.url)}">${escapeHtml(a.title)}</a>（${lang(a.lang)}）`)
    .join("；");
  const notes = [
    g.ai
      ? tr("这部分由 AI（{model}）根据书名、作者和开头片段写成，可能有错误，请以原书为准。", { model: escapeHtml(g.ai.model) }) +
        (g.ai.known ? "" : tr("模型表示不了解这本书，只介绍了开头的设定。"))
      : tr("内容摘自维基百科，按 CC BY-SA 4.0 许可使用：{sources}。", { sources }),
    g.plotHidden ? tr("按设置不剧透：略去了情节梗概和人物介绍。") : "",
    g.grokipedia ? tr("另可参考 Grokipedia（AI 生成的百科，英文）：{link}", { link: `<a href="${escapeHtml(g.grokipedia)}">${escapeHtml(g.grokipedia)}</a>` }) : "",
  ]
    .filter(Boolean)
    .join(" ");
  const title = g.ai ? tr("读前导读 · AI 生成") : tr("读前导读");
  return `<section class="st-guide"><h2>${title}</h2>${parts.join("")}<p class="st-note">${notes}</p></section>`;
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

type Numbered = { w: StudyExportWord; n: number };

/** 按章（书自己的章节标题）或按首字母分组；编号在两份词表里一致 */
function groups(input: StudyExportInput): Array<{ head: string; items: Numbered[] }> {
  const numbered = input.words.map((w, i) => ({ w, n: i + 1 }));
  const out: Array<{ head: string; items: Numbered[] }> = [];
  if (input.order === "chapter") {
    // 用书自己的章节标题：小说正文前常有 Etymology 这类部分，自己编号会和书里的章号对不上
    for (const c of input.chapters) {
      const items = numbered.filter(({ w }) => w.chapter === c.idx);
      if (items.length) out.push({ head: escapeHtml(c.title), items });
    }
  } else {
    for (const x of numbered) {
      const letter = (x.w.lemma[0] ?? "#").toUpperCase();
      const last = out[out.length - 1];
      if (last && last.head === letter) last.items.push(x);
      else out.push({ head: letter, items: [x] });
    }
  }
  return out;
}

function sensesHtml(w: StudyExportWord): string {
  if (w.senses && w.senses.length > 1) return w.senses.map((s, i) => `<span class="gl-sense"><b>${i + 1}</b>${escapeHtml(s)}</span>`).join("");
  return escapeHtml(w.gloss ?? "");
}

/** 简明单词表：编号、单词、词性、中文，一行一个，快速过一遍 */
function simpleList(input: StudyExportInput): string {
  const body = groups(input)
    .map(
      (g) =>
        `<h3>${g.head}<small aria-hidden="true">${tr("{n} 个词", { n: g.items.length })}</small></h3><table class="sl-table"><tbody>${g.items
          .map(({ w, n }) => `<tr><td class="st-n">${n}</td><td class="sl-w">${escapeHtml(w.lemma)}</td><td class="sl-p">${escapeHtml(w.pos ?? "")}</td><td class="sl-z">${sensesHtml(w)}</td></tr>`)
          .join("")}</tbody></table>`,
    )
    .join("");
  return `<section class="st-simple"><h2>${tr("简明单词表")}<small aria-hidden="true">${tr("{n} 个词", { n: input.words.length })}</small></h2>${body}</section>`;
}

function extra(w: StudyExportWord, input: StudyExportInput, chapterTitle: Map<number, string>): string {
  const times = `<span class="gl-form">${tr("全书 {n} 次", { n: w.count })}</span>`;
  if (input.order === "chapter") return times;
  const where = chapterTitle.get(w.chapter);
  return where ? `${times}<span class="gl-form">${tr("首见于 {title}", { title: escapeHtml(where) })}</span>` : times;
}

/** 详细词表：每个词带书中原句和词典释义 */
function detailList(input: StudyExportInput): string {
  const titles = new Map(input.chapters.map((c) => [c.idx, c.title]));
  const item = ({ w, n }: Numbered) => glossItem(w, n, input.options, input.style, extra(w, input, titles));
  const body = groups(input)
    .map((g) => `<section class="glossary st-group"><h3>${g.head}<small aria-hidden="true">${tr("{n} 个词", { n: g.items.length })}</small></h3><div class="gl-list">${g.items.map(item).join("")}</div></section>`)
    .join("");
  return `<section class="st-detail"><h2>${tr("详细词表")}<small aria-hidden="true">${tr("原句与词典释义")}</small></h2>${body}</section>`;
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
  const o = input.options;
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
  const has = input.words.length > 0;
  return `<main class="lens-export lens-article lens-study" data-layout="book" data-style="${style}"><header class="ar-head"><div class="ar-kicker">${tr("读前预习")}</div><h1>${escapeHtml(input.title)}</h1><p class="ar-meta">${meta}</p></header>${input.guides.map(guideSection).join("")}${o.roadmap ? roadmap(input, no) : ""}${o.simple && has ? simpleList(input) : ""}${o.detail && has ? detailList(input) : ""}${o.detail && o.index && input.order === "chapter" && has ? alphaIndex(input) : ""}<footer class="copyright">${tr(COPYRIGHT)}</footer></main>`;
}
