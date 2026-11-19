// 阅读批注（Rust：commands/annotations.rs）

import { invoke } from "@tauri-apps/api/core";

export type AnnotationColor = "yellow" | "green" | "blue" | "pink";
export const ANNOTATION_COLORS: AnnotationColor[] = ["yellow", "green", "blue", "pink"];

export interface Annotation {
  id: number;
  articleId: number;
  /** 第几章（普通文章为 0） */
  chapter: number;
  quote: string;
  prefix: string;
  suffix: string;
  color: AnnotationColor;
  note: string | null;
  createdAt: number;
  updatedAt: number;
}

export interface NewAnnotation {
  articleId: number;
  chapter: number;
  quote: string;
  prefix: string;
  suffix: string;
  color: AnnotationColor;
  note: string | null;
  /** 撤销删除时带回原来的时间 */
  createdAt: number | null;
}

export const listAnnotations = (articleId: number) => invoke<Annotation[]>("annotation_list", { articleId });
export const addAnnotation = (input: NewAnnotation) => invoke<Annotation>("annotation_add", { input });
export const setAnnotationColor = (id: number, color: AnnotationColor) => invoke<Annotation>("annotation_set_color", { id, color });
export const setAnnotationNote = (id: number, note: string | null) => invoke<Annotation>("annotation_set_note", { id, note });
export const deleteAnnotation = (id: number) => invoke<Annotation | null>("annotation_delete", { id });
