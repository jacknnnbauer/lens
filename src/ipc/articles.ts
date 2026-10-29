// 阅读：文章库（Rust：commands/articles.rs、db/articles.rs）。

import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { UnlistenFn } from "@tauri-apps/api/event";

export interface ArticleMeta {
  id: number;
  title: string;
  sourcePath: string | null;
  /** 英文词数 */
  words: number;
  /** 上次读到的位置（0–1） */
  progress: number;
  createdAt: number;
  updatedAt: number;
  openedAt: number | null;
  /** 开头一段纯文字 */
  excerpt: string;
}

export interface Article extends ArticleMeta {
  /** Markdown 原文 */
  body: string;
}

export function listArticles(): Promise<ArticleMeta[]> {
  return invoke<ArticleMeta[]>("article_list");
}

export function openArticle(id: number): Promise<Article | null> {
  return invoke<Article | null>("article_open", { id });
}

export function createArticle(body: string, title?: string): Promise<number> {
  return invoke<number>("article_create", { body, title: title?.trim() || null });
}

export function importArticleFiles(paths: string[]): Promise<{ ids: number[]; failed: Array<[string, string]> }> {
  return invoke("article_import_files", { paths });
}

export function renameArticle(id: number, title: string): Promise<void> {
  return invoke("article_rename", { id, title });
}

/** 改正文（不动导入时的原文件），返回新的词数 */
export function updateArticleBody(id: number, body: string): Promise<number> {
  return invoke<number>("article_update_body", { id, body });
}

export function setArticleProgress(id: number, progress: number): Promise<void> {
  return invoke("article_progress", { id, progress });
}

/** 资源管理器右键「用 Lens 打开」导入、等着打开的文章 id（取走后清空） */
export function takePendingArticle(): Promise<number | null> {
  return invoke<number | null>("article_take_pending");
}

/** Lens 开着时又从资源管理器打开了文章 */
export function onOpenArticle(cb: () => void): Promise<UnlistenFn> {
  return getCurrentWebviewWindow().listen("reader:open", () => cb());
}

export function deleteArticle(id: number): Promise<boolean> {
  return invoke<boolean>("article_delete", { id });
}

/** 存语境句（所在段落 + 词在段落里的 UTF-16 区间），返回语境 id；取不出句子时为 null */
export function saveReadingContext(text: string, paragraph: string, sel16: [number, number], title: string): Promise<number | null> {
  return invoke<number | null>("save_in_app_context", { text, paragraph, sel16, title });
}
