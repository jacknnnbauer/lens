// vocabulary.com 真实例句（Rust：vocab.rs、commands/ai.rs 的 vocab_examples）。

import { invoke } from "@tauri-apps/api/core";

export interface VocabSentence {
  text: string;
  /** 词在句中的位置（字符下标 [起, 止)） */
  hl: [number, number] | null;
  source: string;
  title: string | null;
  author: string | null;
  published: number | null;
  url: string | null;
}

export interface VocabPage {
  word: string;
  domain: string;
  offset: number;
  total: number;
  sentences: VocabSentence[];
  cachedAt: number | null;
  /** 联网失败、显示的是过期缓存 */
  stale: boolean;
}

/** domain：F 小说 · A 艺术文化 · N 新闻 · B 商业 · S 体育 · M 科学医学 · T 科技；空 = 全部 */
export function vocabExamples(word: string, opts: { domain?: string; offset?: number; force?: boolean } = {}): Promise<VocabPage> {
  return invoke<VocabPage>("vocab_examples", { word, domain: opts.domain ?? null, offset: opts.offset ?? 0, force: !!opts.force });
}
