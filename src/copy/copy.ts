// 复制入口（SPEC 10）：单部词典 / 合并所有展开的词典，三种格式；写剪贴板走 Rust（CF_HTML + 纯文本）。

import { invoke } from "@tauri-apps/api/core";
import type { Copy } from "../ipc/settings";
import type { Card } from "../stores/lookup";
import { toast } from "../stores/toast";
import { useAppliedTheme } from "../theme/useApplyTheme";
import { contextMarkdown, escapeHtml, htmlToMarkdown, htmlToText, sourceLine, type ContextMeta, type CopyFormat } from "./convert";
import { basicHtml, inlineHtml, visibleHtml } from "./inlineHtml";
import { tr } from "../i18n";

export const FORMAT_NAME: Record<CopyFormat, string> = { md: "Markdown", html: "HTML", text: "纯文本" };
const FORMAT_HINT: Record<CopyFormat, string> = {
  md: "可粘贴到 Obsidian / Typora",
  html: "可粘贴到 Word / OneNote",
  text: "可粘贴到任意位置",
};

export const DEFAULT_COPY: Copy = {
  defaultFormat: "md",
  appendSource: true,
  includeExamples: true,
  inlineHtmlStyles: true,
  mdImagesAsLinks: true,
};

interface Payload {
  text: string;
  html: string | null;
}

/** 已渲染的词条正文（DictBody 的 shadowRoot），按词条顺序。卡片折叠时为空。 */
function renderedBodies(dictId: string): ShadowRoot[] {
  return Array.from(document.querySelectorAll<HTMLElement>(`[data-dict-body="${CSS.escape(dictId)}"]`))
    .map((h) => h.shadowRoot)
    .filter((r): r is ShadowRoot => r !== null);
}

function cardPayload(card: Card, format: CopyFormat, cfg: Copy, heading: boolean): Payload {
  const opts = { includeExamples: cfg.includeExamples, imagesAsLinks: cfg.mdImagesAsLinks };
  const headword = card.entries[0]?.headword ?? "";
  const src = { dict: card.name, headword };

  if (format === "html") {
    const dark = useAppliedTheme.getState().dark;
    const inline = { colors: cfg.inlineHtmlStyles && !dark, includeExamples: cfg.includeExamples };
    const bodies = renderedBodies(card.id);
    const parts = card.entries.map((e, i) =>
      cfg.inlineHtmlStyles && bodies[i] ? inlineHtml(bodies[i], inline) : basicHtml(e.html, inline),
    );
    let html = parts.join("<hr>");
    if (heading) html = `<h2>${escapeHtml(card.name)}</h2>${html}`;
    if (cfg.appendSource) html += sourceLine("html", src);
    const text = card.entries.map((e) => htmlToText(e.html, opts)).join("\n\n");
    return { html, text: (heading ? `${card.name}\n` : "") + text };
  }

  const convert = format === "md" ? htmlToMarkdown : htmlToText;
  // 已渲染时用「看得见」的内容（去掉词典 CSS 隐藏的元素），否则用原始词条
  const rendered = renderedBodies(card.id);
  const sources = card.entries.map((e, i) => (rendered[i] ? visibleHtml(rendered[i]) : e.html));
  let body = sources.map((h) => convert(h, opts)).join(format === "md" ? "\n\n---\n\n" : "\n\n");
  if (heading) body = format === "md" ? `## ${card.name}\n\n${body}` : `【${card.name}】\n${body}`;
  if (cfg.appendSource) body += `\n\n${sourceLine(format, src)}`;
  return { text: body, html: null };
}

async function write(p: Payload) {
  await invoke("write_clipboard", { text: p.text, html: p.html });
}

/** 复制一部词典；context 给出时（弹窗划词）在 Markdown / 纯文本末尾附上语境句。 */
export async function copyCard(card: Card, format: CopyFormat, cfg: Copy, context?: ContextMeta | null) {
  if (card.status !== "hit") return;
  try {
    const p = cardPayload(card, format, cfg, false);
    if (context && format === "md") p.text += `\n\n${contextMarkdown(context)}`;
    if (context && format === "text") p.text += `\n\n“${context.sentence}”`;
    await write(p);
    toast(tr("已复制「{name}」为 {format}", { name: card.name, format: tr(FORMAT_NAME[format]) }), tr(FORMAT_HINT[format]));
  } catch (e) {
    toast(tr("复制失败：{error}", { error: String(e) }));
  }
}

/** 合并复制所有展开且命中的词典（Ctrl+Shift+A）。 */
export async function copyMerged(cards: Card[], format: CopyFormat, cfg: Copy) {
  const hits = cards.filter((c) => c.status === "hit");
  if (hits.length === 0) return;
  try {
    const parts = hits.map((c) => cardPayload(c, format, cfg, true));
    const sep = format === "md" ? "\n\n" : "\n\n";
    await write({
      text: parts.map((p) => p.text).join(sep),
      html: format === "html" ? parts.map((p) => p.html ?? "").join("") : null,
    });
    toast(tr("已合并复制 {n} 部词典为 {format}", { n: hits.length, format: tr(FORMAT_NAME[format]) }), tr(FORMAT_HINT[format]));
  } catch (e) {
    toast(tr("复制失败：{error}", { error: String(e) }));
  }
}
