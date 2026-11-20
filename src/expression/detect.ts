// 英文怎么说：搜索框里的内容能不能用、算中文意思还是英文句子。

import { isSentence } from "../components/sentence";

const HAN = /\p{Script=Han}/u;

/** 含汉字 → 中文意思（多短都行）；英文要是一整句才算；只有空白、标点时不出现 */
export function expressionMode(text: string): "zh" | "en" | null {
  const t = text.trim();
  if (!/[\p{L}\p{N}]/u.test(t)) return null;
  if (HAN.test(t)) return "zh";
  return isSentence(t) ? "en" : null;
}

/** 输入最长多少字（和 Rust 的 expression_ai::MAX_CHARS 一致） */
export const MAX_CHARS = 600;

export function tooLong(text: string): boolean {
  return [...text.trim()].length > MAX_CHARS;
}
