// 精读本的排版：放在全书预习包的最前面，读完就可以不读原书。
// 顺序按《How to Read a Book》分析阅读：整体（含阅读建议）→ 人物与世界 → 情节时间线 → 名场面（整段原文）
// → 逐章复述（带书中原文）→ 主题与写作
// → 评价与影响 → 对话准备 → 细节自测（答案放在后面）。各部分标题是 h2，逐章提要的每章是 h3，PDF 里有书签。

import { escapeHtml } from "../copy/convert";
import { tr } from "../i18n";
import type { Digest, DigestChapter, DigestQuote } from "../ipc/digest";
import { diagramsHtml } from "./diagrams";

function paras(text: string, cls = ""): string {
  const c = cls ? ` class="${cls}"` : "";
  return text
    .split("\n")
    .map((p) => p.trim())
    .filter(Boolean)
    .map((p) => `<p${c}>${escapeHtml(p)}</p>`)
    .join("");
}

/** 章的简称：标题里有数字就是「第 N 章」，否则用标题（Epilogue、Etymology…） */
export function chapterLabel(title: string): string {
  const m = /\b(\d{1,4})\b/.exec(title);
  if (m) return tr("第 {n} 章", { n: Number(m[1]) });
  return title.length > 28 ? `${title.slice(0, 26)}…` : title;
}

const ADVICE: Record<DigestChapter["advice"], string> = {
  read: "值得读原文",
  normal: "",
  skim: "可略读",
  skip: "可跳过",
};

export function buildDigestSections(d: Digest): string {
  const s = d.synthesis;
  const titles = new Map(d.chapters.map((c) => [c.idx, c.title]));
  const quotes = new Map(d.quotes.map((q) => [q.id, q]));
  const ref = (idx: number) => {
    const t = titles.get(idx);
    return t ? escapeHtml(chapterLabel(t)) : "";
  };
  const refs = (list: number[]) => {
    const r = list.map(ref).filter(Boolean);
    return r.length ? `<span class="dg-ref">${r.join("、")}</span>` : "";
  };
  const quote = (q: DigestQuote, withWhy = true) =>
    `<blockquote class="dg-q"><p class="dg-en">${escapeHtml(q.text)}</p>${q.zh ? `<p class="dg-zh">${escapeHtml(q.zh)}</p>` : ""}${
      withWhy && q.why ? `<p class="dg-why">${escapeHtml(q.why)}<span class="dg-ref">${ref(q.chapter)}</span></p>` : withWhy ? "" : `<p class="dg-why"><span class="dg-ref">${ref(q.chapter)}</span></p>`
    }</blockquote>`;
  const sec = (cls: string, title: string, sub: string, body: string) =>
    body.trim() ? `<section class="dg ${cls}"><h2>${title}${sub ? `<small aria-hidden="true">${sub}</small>` : ""}</h2>${body}</section>` : "";
  const h3 = (t: string, body: string) => (body.trim() ? `<h3>${t}</h3>${body}` : "");

  const note = `<p class="st-note dg-note">${tr("精读本由 AI（{model}）通读全书写成，可能有错误；书中原文都在原书里逐字核对过。", { model: escapeHtml(d.model) })}${
    d.dropped > 0 ? tr("另有 {n} 段模型给出的引文在书里找不到，已丢弃。", { n: d.dropped }) : ""
  }${
    d.scenes.length === 0
      ? ""
      : d.publicDomain
        ? tr("作者卒于 {year} 年，这本书已进入公有领域，名场面放了整段原文。", { year: d.authorDied ?? "" })
        : tr("这本书可能仍在版权期内（或查不到作者卒年），名场面只放开头几句原文。")
  }</p>`;
  // 阅读建议：想读一部分原文时读哪些章、哪些可以略过
  const pick = (a: DigestChapter["advice"]) => d.chapters.filter((c) => c.advice === a);
  const mustRead = pick("read");
  const skim = pick("skim");
  const skip = pick("skip");
  const advice =
    mustRead.length + skim.length + skip.length === 0
      ? ""
      : `<p class="st-note">${tr("读完这份精读本已经可以不读原书；想读一部分原文时，按下面的建议挑。")}</p>${
          mustRead.length
            ? `<dl class="dg-dl dg-advice">${mustRead.map((c) => `<dt>${escapeHtml(c.title)}</dt><dd>${escapeHtml(c.adviceWhy)}</dd>`).join("")}</dl>`
            : ""
        }${skim.length ? `<p class="dg-adv-list"><b>${tr("可以略读")}</b>${skim.map((c) => ref(c.idx)).join("、")}</p>` : ""}${
          skip.length ? `<p class="dg-adv-list"><b>${tr("可以跳过")}</b>${skip.map((c) => ref(c.idx)).join("、")}</p>` : ""
        }`;
  // 一、整体
  const structure = s.structure.length
    ? `<table class="st-table dg-structure"><thead><tr><th>${tr("部分")}</th><th>${tr("章节")}</th><th>${tr("讲什么")}</th></tr></thead><tbody>${s.structure
        .map((p) => `<tr><td class="dg-strong">${escapeHtml(p.title)}</td><td class="dg-range">${ref(p.from)}${p.to !== p.from ? ` – ${ref(p.to)}` : ""}</td><td>${escapeHtml(p.summary)}</td></tr>`)
        .join("")}</tbody></table>`
    : "";
  const whole = sec(
    "dg-whole",
    tr("整体"),
    tr("这本书讲什么"),
    (s.oneLine ? `<p class="dg-one">${escapeHtml(s.oneLine)}</p>` : "") +
      h3(tr("一段话梗概"), paras(s.paragraph)) +
      h3(tr("三分钟讲解稿"), s.talk.trim() ? `<div class="dg-talk">${paras(s.talk)}</div>` : "") +
      h3(tr("全书结构"), structure) +
      h3(tr("阅读建议"), advice) +
      note,
  );

  // 二、人物与世界
  const people = s.characters
    .map((c) => {
      const q = c.quote ? quotes.get(c.quote) : undefined;
      const row = (k: string, v: string) => (v.trim() ? `<p><b>${k}</b>${escapeHtml(v)}</p>` : "");
      return `<div class="dg-card"><h4>${escapeHtml(c.name)}${c.role ? `<small>${escapeHtml(c.role)}</small>` : ""}</h4>${row(tr("经历"), c.arc)}${row(tr("关键场景"), c.scenes)}${row(tr("象征与作用"), c.meaning)}${q ? quote(q, false) : ""}</div>`;
    })
    .join("");
  const glossary = s.glossary.length
    ? `<table class="sl-table dg-terms"><tbody>${s.glossary.map((t) => `<tr><td class="sl-w">${escapeHtml(t.term)}</td><td class="dg-strong">${escapeHtml(t.zh)}</td><td>${escapeHtml(t.note)}</td></tr>`).join("")}</tbody></table>`
    : "";
  const world = sec(
    "dg-people",
    tr("人物与世界"),
    tr("{n} 个人物", { n: s.characters.length }),
    h3(tr("主要人物"), `<div class="dg-cards">${people}</div>`) + h3(tr("人物关系"), paras(s.relations)) + h3(tr("故事的世界"), paras(s.background)) + h3(tr("术语"), glossary),
  );

  // 三、情节：时间线、逐章提要（每章一个书签）
  const timeline = s.timeline.length
    ? `<table class="st-table dg-timeline"><tbody>${s.timeline.map((e, i) => `<tr><td class="st-n">${i + 1}</td><td>${escapeHtml(e.event)}</td><td class="st-num">${ref(e.chapter)}</td></tr>`).join("")}</tbody></table>`
    : "";
  const parts = new Map<number, string>();
  for (const p of s.structure) if (!parts.has(p.from)) parts.set(p.from, p.title);
  const sceneOf = new Map(d.scenes.map((x) => [x.chapter, x]));
  const chapters = d.chapters
    .map((c) => {
      const part = parts.get(c.idx);
      const events = c.events.length ? `<ul class="dg-events">${c.events.map((e) => `<li>${escapeHtml(e)}</li>`).join("")}</ul>` : "";
      const qs = c.quotes.map((id) => quotes.get(id)).filter((q): q is DigestQuote => !!q);
      const tags = [c.kind === "digression" ? tr("题外话") : "", ADVICE[c.advice] ? tr(ADVICE[c.advice]) : ""].filter(Boolean).join(" · ");
      const scene = sceneOf.get(c.idx);
      const why = c.advice !== "normal" && c.adviceWhy ? `<p class="dg-adv">${tr(ADVICE[c.advice])}：${escapeHtml(c.adviceWhy)}</p>` : "";
      const link = scene ? `<p class="dg-adv">${tr("本章有名场面 {n}：{title}", { n: scene.id, title: escapeHtml(scene.title) })}</p>` : "";
      return `${part ? `<div class="dg-part">${escapeHtml(part)}</div>` : ""}<div class="dg-ch" data-kind="${c.kind}" data-advice="${c.advice}"><h3>${escapeHtml(c.title)}${tags ? `<small aria-hidden="true">${tags}</small>` : ""}</h3>${paras(c.summary)}${events}${why}${link}${qs.map((q) => quote(q)).join("")}</div>`;
    })
    .join("");
  const plot = sec("dg-timeline-sec", tr("情节时间线"), tr("{n} 个关键事件", { n: s.timeline.length }), timeline);
  const byChapter = sec("dg-chapters", tr("逐章复述"), tr("{n} 章，原文 {q} 段", { n: d.chapters.length, q: d.quotes.length }), `<p class="st-note">${tr("引用的英文都逐字取自原书；标「题外话」的章节以议论、知识为主，不推进情节。")}</p>${chapters}`);

  // 名场面：整段原文（非公版书只有开头）、译文、前因后果
  const scenes = d.scenes
    .map(
      (x) =>
        `<div class="dg-scene"><h3>${tr("名场面 {n}", { n: x.id })} · ${escapeHtml(x.title)}<small aria-hidden="true">${ref(x.chapter)}</small></h3>${
          x.context ? `<p class="dg-context"><b>${tr("前因后果")}</b>${escapeHtml(x.context)}</p>` : ""
        }<blockquote class="dg-q dg-long">${paras(x.text, "dg-en")}${x.clipped ? `<p class="dg-why">${tr("（仅开头，以下从略）")}</p>` : ""}</blockquote>${
          x.zh ? `<div class="dg-zh-long">${paras(x.zh)}</div>` : ""
        }${x.why ? `<p class="dg-why">${escapeHtml(x.why)}</p>` : ""}</div>`,
    )
    .join("");
  const famous = sec("dg-scenes", tr("名场面"), tr("{n} 段原文", { n: d.scenes.length }), scenes);

  // 四、主题与写作
  const themes = s.themes.map((t) => `<h3>${escapeHtml(t.name)}</h3>${paras(t.text)}${refs(t.chapters) ? `<p class="dg-where">${tr("依据：")}${refs(t.chapters)}</p>` : ""}`).join("");
  const motifs = s.motifs.length
    ? `<dl class="dg-dl">${s.motifs.map((m) => `<dt>${escapeHtml(m.name)}</dt><dd>${escapeHtml(m.meaning)}${refs(m.chapters)}</dd>`).join("")}</dl>`
    : "";
  const ideas = sec("dg-themes", tr("主题与写作"), "", themes + h3(tr("关键意象"), motifs) + h3(tr("写作手法"), paras(s.technique)));

  // 五、评价与影响
  const debates = s.criticism.map((c) => `<div class="dg-debate"><p class="dg-strong">${escapeHtml(c.claim)}</p><p><b>${tr("支持")}</b>${escapeHtml(c.pro)}</p><p><b>${tr("反驳")}</b>${escapeHtml(c.con)}</p></div>`).join("");
  const further = s.further.length
    ? `<dl class="dg-dl">${s.further.map((b) => `<dt>${escapeHtml(b.title)}${b.author ? `<small>${escapeHtml(b.author)}</small>` : ""}</dt><dd>${escapeHtml(b.why)}</dd>`).join("")}</dl>`
    : "";
  const fame = sec("dg-reception", tr("评价与影响"), "", h3(tr("出版与评价史"), paras(s.reception)) + h3(tr("争议与批评"), debates) + h3(tr("原型与影响"), paras(s.origins)) + h3(tr("延伸阅读"), further));

  // 六、对话准备
  const faq = s.faq.map((f, i) => `<div class="dg-faq"><p class="dg-strong">${i + 1}. ${escapeHtml(f.q)}</p><p>${escapeHtml(f.a)}${refs(f.chapters)}</p></div>`).join("");
  const myths = s.misconceptions.map((m) => `<div class="dg-faq"><p><b>${tr("误")}</b>${escapeHtml(m.wrong)}</p><p><b>${tr("正")}</b>${escapeHtml(m.right)}</p></div>`).join("");
  const topics = s.topics.map((t) => `<div class="dg-debate"><p class="dg-strong">${escapeHtml(t.topic)}</p><p><b>${tr("观点")}</b>${escapeHtml(t.view)}</p><p><b>${tr("有人会说")}</b>${escapeHtml(t.counter)}</p></div>`).join("");
  const golden = s.golden.map((id) => quotes.get(id)).filter((q): q is DigestQuote => !!q).map((q) => quote(q, false)).join("");
  const talk = sec("dg-talk-sec", tr("对话准备"), tr("跟人聊这本书"), h3(tr("最可能被问到的问题"), faq) + h3(tr("常见误区"), myths) + h3(tr("可以展开的话题"), topics) + h3(tr("可以随口引用的原文"), golden));

  // 七、细节自测：先题后答
  const quiz = s.quiz.length
    ? `<ol class="dg-quiz">${s.quiz.map((q) => `<li>${escapeHtml(q.q)}</li>`).join("")}</ol><h3>${tr("答案")}</h3><ol class="dg-quiz dg-answers">${s.quiz.map((q) => `<li>${escapeHtml(q.a)}<span class="dg-ref">${ref(q.chapter)}</span></li>`).join("")}</ol>`
    : "";
  const test = sec("dg-test", tr("细节自测"), tr("{n} 题", { n: s.quiz.length }), quiz);

  // 图解：AI 选的几张图（人物关系、伏笔、等级、路线、节奏、对照…）
  const figures = s.diagrams
    ? diagramsHtml(
        s.diagrams,
        d.chapters,
        s.structure.map((p) => ({ title: p.title, from: p.from, to: p.to })),
        (idx) => (titles.has(idx) ? chapterLabel(titles.get(idx)!) : ""),
      )
    : "";
  const visual = sec("dg-figures", tr("图解"), tr("{n} 张图", { n: (figures.match(/class="dg-figure"/g) ?? []).length }), figures);

  return whole + visual + world + plot + famous + byChapter + ideas + fame + talk + test;
}
