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

export function bookStudyList(id: number, level: number, minCount: number, includeSaved: boolean, scope: Scope, order: StudyOrder): Promise<StudyList> {
  return invoke<StudyList>("book_study_list", { id, level, minCount, includeSaved, scope, order });
}
