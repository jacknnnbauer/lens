// 全书预习：整本书的生词按门槛挑选、每章统计（后端 commands/study.rs，第一次分析后缓存）。

import { invoke } from "@tauri-apps/api/core";

export interface LevelCount {
  level: number;
  /** 生词数（不含已收藏） */
  words: number;
  saved: number;
  /** 认识的词次占比 */
  coverage: number;
}

export interface StudyPreview {
  title: string;
  author: string | null;
  chapters: number;
  total: number;
  unique: number;
  minutes: number;
  levels: LevelCount[];
  source: "coca" | "builtin";
  /** 书里有正文的各部分，和实际用的范围（含两端） */
  parts: Part[];
  from: number;
  to: number;
}

export interface Part {
  /** 书里的章节序号 */
  idx: number;
  title: string;
  /** 书前书后的附加内容（前言、注释…） */
  extra: boolean;
}

/** 预习范围：从哪部分到哪部分（不给就用默认范围：正文，去掉出版社附赠的内容），范围里的附加内容算不算 */
export interface Scope {
  from: number | null;
  to: number | null;
  includeExtras: boolean;
}

export interface StudyWord {
  lemma: string;
  forms: string[];
  count: number;
  rank: number | null;
  status: "new" | "saved";
  /** 第一次出现的章（书里的章节序号） */
  chapter: number;
  sentence: string | null;
  hlStart: number;
  hlEnd: number;
}

export interface ChapterRow {
  idx: number;
  title: string;
  tokens: number;
  minutes: number;
  /** 这一章第一次出现的生词数 */
  firstWords: number;
  newTokens: number;
  /** 每千词里的生词次数 */
  density: number;
}

export interface StudyList {
  words: StudyWord[];
  chapters: ChapterRow[];
}

export type StudyOrder = "chapter" | "alpha";

export function bookStudyPreview(id: number, minCount: number, scope: Scope): Promise<StudyPreview> {
  return invoke<StudyPreview>("book_study_preview", { id, minCount, scope });
}

/** `limit`：最多要多少个（书里出现次数多的优先）；null = 不限 */
export function bookStudyList(id: number, level: number, minCount: number, includeSaved: boolean, scope: Scope, order: StudyOrder, limit: number | null): Promise<StudyList> {
  return invoke<StudyList>("book_study_list", { id, level, minCount, includeSaved, scope, order, limit });
}

export interface GuideSection {
  kind: "lead" | "background" | "characters" | "plot" | "themes" | "reception" | "other";
  /** 条目里的原标题（简介为空） */
  heading: string;
  text: string;
}

export interface GuideArticle {
  lang: "zh" | "en";
  title: string;
  url: string;
  sections: GuideSection[];
}

export interface BookGuide {
  book: GuideArticle;
  author: GuideArticle | null;
  /** 按设置不剧透，略去了情节梗概和人物 */
  plotHidden: boolean;
  /** 这本书在 Grokipedia 上的地址：只给链接，它的条款不允许程序抓取内容；AI 导读为空 */
  grokipedia: string;
  /** AI 写的导读：模型、模型说它知不知道这本书；维基百科的为 null */
  ai: { model: string; known: boolean } | null;
}

/** 读前导读：用书名和作者查维基百科（联网，只发书名和作者）；找不到时为 null。剧透、语言按设置 › 全书预习包 */
export function bookGuide(id: number): Promise<BookGuide | null> {
  return invoke<BookGuide | null>("book_guide", { id });
}

/** AI 写的读前导读（设置 › AI 例句里的模型，按用量计费）。`cachedOnly`：只取存过的，不花钱；`force`：重新生成 */
export function bookGuideAi(id: number, cachedOnly: boolean, force = false): Promise<BookGuide | null> {
  return invoke<BookGuide | null>("book_guide_ai", { id, cachedOnly, force });
}
