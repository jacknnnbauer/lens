// 收藏当前词（Ctrl+S）：主窗口与弹窗共用。简释取首个命中词典的摘要（去掉开头的词头）。

import { useEffect, useState } from "react";
import { saveWord, wordId } from "../ipc/words";
import type { Card } from "./lookup";
import { toast } from "./toast";
import { tr } from "../i18n";

const BRIEF_CHARS = 80;

/** 词频、搭配类词典（COCA 词频表之类）：摘要是数字和搭配词，不适合当简释（与 Rust 的 is_reference_dict 一致） */
export function isReferenceDict(name: string): boolean {
  const n = name.toLowerCase();
  return ["coca", "colloc", "搭配", "词频", "frequen", "同义", "thesaurus", "词根"].some((k) => n.includes(k));
}

/** 摘要像不像释义：字母和汉字至少占四成（排除「v 11677 1278150678 n 1572…」这种词频数字） */
export function looksLikeGloss(s: string): boolean {
  const chars = [...s.replace(/\s/g, "")];
  if (chars.length === 0) return false;
  const words = chars.filter((c) => /[\p{L}]/u.test(c)).length;
  return words / chars.length >= 0.4;
}

/** 一句话释义：取第一本像样的命中词典（跳过词频、搭配类）的摘要，去掉开头重复的词头，截到 BRIEF_CHARS。 */
export function briefOf(cards: Card[], word: string): string | null {
  const summary = cards
    .filter((c) => c.status === "hit" && !isReferenceDict(c.name))
    .map((c) => c.entries[0]?.summary ?? "")
    .find((s) => looksLikeGloss(s));
  if (!summary) return null;
  let s = summary.replace(/…$/, "").trim();
  const lower = s.toLowerCase();
  const w = word.toLowerCase();
  if (w && lower.startsWith(w)) s = s.slice(w.length).trim();
  s = s.replace(/^[\s·:：,，;；-]+/, "");
  return s.length > BRIEF_CHARS ? `${s.slice(0, BRIEF_CHARS)}…` : s || null;
}

/** 当前查询词的收藏状态与收藏动作。 */
export function useSaveWord(query: string | undefined, cards: Card[], contextId: number | null = null) {
  const [savedId, setSavedId] = useState<number | null>(null);

  useEffect(() => {
    setSavedId(null);
    if (!query) return;
    let alive = true;
    wordId(query).then((id) => alive && setSavedId(id), () => undefined);
    return () => {
      alive = false;
    };
  }, [query]);

  const save = async () => {
    if (!query || !cards.some((c) => c.status === "hit")) return;
    try {
      const id = await saveWord(query, { brief: briefOf(cards, query), contextId });
      const already = savedId !== null;
      setSavedId(id);
      toast(already ? tr("「{q}」已在生词本，已追加语境", { q: query }) : tr("已收藏「{q}」", { q: query }), tr("生词本 Ctrl+B"));
    } catch (e) {
      toast(tr("收藏失败：{error}", { error: String(e) }));
    }
  };

  return { saved: savedId !== null, save };
}
