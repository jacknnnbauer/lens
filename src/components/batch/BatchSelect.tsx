// 批量导出 · 选词（画板 10）：从历史 / 粘贴词表 / 生词本选词，设置输出，预检后开始。

import { documentDir, join } from "@tauri-apps/api/path";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useEffect, useMemo, useRef, useState } from "react";
import type { ExportLayout, ExportStyle, Paper } from "../../export-templates/template";
import { isoDate } from "../../export-templates/template";
import { batchParse, batchPreflight, startBatch, type BatchRequest, type BatchSource, type ParseOutput, type Preflight } from "../../ipc/batch";
import { listHistory, type HistoryItem } from "../../ipc/history";
import { listTags, listWords, type TagCount, type Word } from "../../ipc/words";
import { useBatch } from "../../stores/batch";
import { useDicts } from "../../stores/dicts";
import { useSettings } from "../../stores/settings";
import { useAppliedTheme } from "../../theme/useApplyTheme";
import styles from "./Batch.module.css";
import { isStopword } from "./stopwords";
import { VirtualList } from "./VirtualList";

const ROW = 36;
const DAY = 86_400_000;
const SOURCE_NAME: Record<BatchSource, string> = { history: "历史", paste: "词表", wordbook: "生词本" };

type Range = "today" | "7d" | "month" | "all";
const RANGES: Array<[Range, string]> = [
  ["today", "今天"],
  ["7d", "最近 7 天"],
  ["month", "本月"],
  ["all", "全部"],
];

/** 列表里的一行：日期分组标题或一个词 */
type Row = { kind: "head"; label: string } | { kind: "word"; key: string; word: string; brief: string; count: number; stop: boolean };

function rangeStart(r: Range): number | undefined {
  const now = new Date();
  const midnight = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  if (r === "today") return midnight;
  if (r === "7d") return midnight - 6 * DAY;
  if (r === "month") return new Date(now.getFullYear(), now.getMonth(), 1).getTime();
  return undefined;
}

function dayLabel(ms: number): string {
  const d = new Date(ms);
  const today = new Date();
  const diff = Math.round((new Date(today.getFullYear(), today.getMonth(), today.getDate()).getTime() - new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime()) / DAY);
  const md = `${d.getMonth() + 1} 月 ${d.getDate()} 日`;
  return diff === 0 ? `今天 · ${md}` : diff === 1 ? `昨天 · ${md}` : `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

/** 历史：同一个词（按原形）只留最近一次，按日期分组 */
function historyRows(items: HistoryItem[]): Row[] {
  const seen = new Set<string>();
  const rows: Row[] = [];
  let lastDay = "";
  for (const h of items) {
    const key = h.lemma || h.query.toLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    const day = dayLabel(h.createdAt);
    if (day !== lastDay) {
      rows.push({ kind: "head", label: day });
      lastDay = day;
    }
    const stop = isStopword(h.query);
    const ctx = h.context?.sentence ? `语境：${h.context.sentence}` : "";
    rows.push({ kind: "word", key, word: h.query, brief: stop ? "常见虚词 · 已自动取消勾选" : ctx, count: h.lookups, stop });
  }
  return rows;
}

function wordbookRows(words: Word[]): Row[] {
  return words.map((w) => ({
    kind: "word",
    key: w.lemma.toLowerCase(),
    word: w.lemma,
    brief: [w.brief, w.tags.length ? w.tags.map((t) => `#${t}`).join(" ") : ""].filter(Boolean).join(" · "),
    count: w.lookups,
    stop: false,
  }));
}

function fmtDuration(sec: number): string {
  if (sec < 60) return `约 ${Math.max(1, sec)} 秒`;
  const m = Math.round(sec / 60);
  return m < 60 ? `约 ${m} 分钟` : `约 ${Math.floor(m / 60)} 小时 ${m % 60} 分`;
}

export function fmtBytes(b: number): string {
  if (b < 1024 * 1024) return `${Math.max(1, Math.round(b / 1024))} KB`;
  if (b < 1024 ** 3) return `${(b / 1024 / 1024).toFixed(b < 10 * 1024 * 1024 ? 1 : 0)} MB`;
  return `${(b / 1024 ** 3).toFixed(1)} GB`;
}

export function BatchSelect({ onClose }: { onClose: () => void }) {
  const initialSource = useBatch((s) => s.initialSource);
  const show = useBatch((s) => s.show);
  const prefs = useSettings((s) => s.settings?.export);
  const accent = useSettings((s) => s.settings?.appearance.accent) ?? "blue";
  const activeProfile = useSettings((s) => s.settings?.activeProfile);
  const profiles = useDicts((s) => s.profiles);
  const theme = useAppliedTheme((s) => s.theme) ?? "paper";

  const [source, setSource] = useState<BatchSource>(initialSource);
  // 历史
  const [range, setRange] = useState<Range>("7d");
  const [history, setHistory] = useState<HistoryItem[]>([]);
  // 生词本
  const [tags, setTags] = useState<TagCount[]>([]);
  const [tag, setTag] = useState("");
  const [words, setWords] = useState<Word[]>([]);
  // 粘贴
  const [text, setText] = useState("");
  // 勾选（历史、生词本）：取消勾选的 key
  const [unchecked, setUnchecked] = useState<Set<string>>(new Set());
  const lastClick = useRef<number | null>(null);
  // 输出
  const [pdf, setPdf] = useState(true);
  const [html, setHtml] = useState(false);
  const [merge, setMerge] = useState(false);
  const [layout, setLayout] = useState<ExportLayout>(prefs?.layout ?? "cards");
  const [style, setStyle] = useState<ExportStyle>(prefs?.style ?? "paperWhite");
  const [paper, setPaper] = useState<Paper>(prefs?.paper ?? "a4");
  const [offlineOnly, setOfflineOnly] = useState(false);
  const [baseDir, setBaseDir] = useState<string | null>(null);
  const [customDir, setCustomDir] = useState<string | null>(null);
  // 整理与预检
  const [parsed, setParsed] = useState<ParseOutput | null>(null);
  const [preflight, setPreflight] = useState<Preflight | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void (async () => {
      try {
        setBaseDir(prefs?.dir ?? (await join(await documentDir(), "Lens导出")));
      } catch {
        setBaseDir(prefs?.dir ?? null);
      }
    })();
    listTags().then(setTags, () => setTags([]));
    // 只在打开时取一次
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (source !== "history") return;
    listHistory({ from: rangeStart(range), limit: 20000 }).then(setHistory, (e) => setError(String(e)));
  }, [source, range]);

  useEffect(() => {
    if (source !== "wordbook") return;
    listWords({ tag: tag || undefined, limit: 20000 }).then(setWords, (e) => setError(String(e)));
  }, [source, tag]);

  const rows: Row[] = useMemo(() => (source === "history" ? historyRows(history) : source === "wordbook" ? wordbookRows(words) : []), [source, history, words]);
  const wordRows = useMemo(() => rows.filter((r): r is Extract<Row, { kind: "word" }> => r.kind === "word"), [rows]);
  const isChecked = (r: Extract<Row, { kind: "word" }>) => !unchecked.has(r.key) && !(r.stop && !unchecked.has(`+${r.key}`));
  const selectedWords = useMemo(() => wordRows.filter(isChecked).map((r) => r.word), [wordRows, unchecked]); // eslint-disable-line react-hooks/exhaustive-deps

  const toggle = (r: Extract<Row, { kind: "word" }>, on: boolean) =>
    setUnchecked((s) => {
      const n = new Set(s);
      if (r.stop) {
        // 虚词默认不勾选，用 +key 记「手动勾上」
        if (on) n.add(`+${r.key}`);
        else n.delete(`+${r.key}`);
      } else if (on) n.delete(r.key);
      else n.add(r.key);
      return n;
    });

  const setAll = (on: boolean) => {
    const n = new Set<string>();
    for (const r of wordRows) {
      if (r.stop && on) n.add(`+${r.key}`);
      if (!r.stop && !on) n.add(r.key);
    }
    setUnchecked(n);
  };

  // 选中的词 / 粘贴的文字 → 整理（去标点、按原形去重、无效条目）
  const parseInput = source === "paste" ? text : selectedWords.join("\n");
  useEffect(() => {
    const t = setTimeout(() => {
      if (!parseInput.trim()) {
        setParsed(null);
        return;
      }
      batchParse(parseInput).then(setParsed, (e) => setError(String(e)));
    }, source === "paste" ? 300 : 150);
    return () => clearTimeout(t);
  }, [parseInput, source]);

  const finalWords = parsed?.words ?? [];
  const label = `${isoDate(new Date())}_${SOURCE_NAME[source]}${source === "wordbook" && tag ? `-${tag}` : ""}_${finalWords.length}词`;
  const [outDir, setOutDir] = useState("");
  useEffect(() => {
    if (customDir) setOutDir(customDir);
    else if (baseDir) void join(baseDir, label).then(setOutDir);
  }, [customDir, baseDir, label]);

  const req: BatchRequest | null =
    finalWords.length > 0 && outDir
      ? {
          source,
          label,
          words: finalWords,
          pdf,
          html,
          merge,
          layout,
          style,
          paper,
          offlineOnly,
          outDir,
          options: { phonetics: true, examples: true, context: true, sourceDate: true },
          theme,
          accent,
        }
      : null;

  // 预检（防抖）
  const reqKey = req ? JSON.stringify({ ...req, words: req.words.length, first: req.words[0] }) : "";
  useEffect(() => {
    if (!req) {
      setPreflight(null);
      return;
    }
    const t = setTimeout(() => batchPreflight(req).then(setPreflight, (e) => setError(String(e))), 400);
    return () => clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [reqKey]);

  const start = async () => {
    if (!req || busy) return;
    setBusy(true);
    setError(null);
    try {
      const id = await startBatch(req);
      await show(id);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const profileName = profiles.find((p) => p.id === activeProfile);
  const canStart = !!req && (pdf || html) && (preflight?.writable ?? true) && !busy;
  const mergedCount = parsed ? parsed.merged.length : 0;

  return (
    <>
      <div className={styles.head}>
        <h2 id="batch-title">批量导出</h2>
        <div role="tablist" aria-label="选词方式" className={styles.tabs}>
          {(["history", "paste", "wordbook"] as const).map((s) => (
            <button key={s} type="button" role="tab" aria-selected={source === s} onClick={() => setSource(s)}>
              {s === "history" ? "从历史记录" : s === "paste" ? "粘贴词表" : "从生词本"}
            </button>
          ))}
        </div>
        <button type="button" className={styles.close} aria-label="关闭" onClick={onClose}>
          ×
        </button>
      </div>

      <div className={styles.body}>
        <div className={styles.left}>
          {source === "paste" ? (
            <div className={styles.paste}>
              <label className={styles.pasteLabel}>
                <span>粘贴的词表</span>
                <textarea
                  value={text}
                  onChange={(e) => setText(e.target.value)}
                  placeholder={"每行一个词，也可以用逗号、分号、顿号、Tab 分隔\nubiquitous\nephemeral, pervasive\ntake into account"}
                  spellCheck={false}
                />
              </label>
              {parsed && (
                <div className={styles.parse}>
                  <strong>解析结果</strong>
                  <span>
                    <span>识别到</span>
                    <span>{parsed.total} 条</span>
                  </span>
                  <span>
                    <span>合并重复 / 同原形</span>
                    <span>
                      −{parsed.merged.length}
                      {parsed.merged[0] && `（${parsed.merged[0].from} → ${parsed.merged[0].to}）`}
                    </span>
                  </span>
                  <span>
                    <span>无效条目</span>
                    <span>
                      −{parsed.invalid.length}
                      {parsed.invalid[0] && `（如 ${parsed.invalid[0].text}：${parsed.invalid[0].reason}）`}
                    </span>
                  </span>
                  {parsed.overLimit.length > 0 && (
                    <span className={styles.warn}>
                      <span>超出单批上限</span>
                      <span>−{parsed.overLimit.length}</span>
                    </span>
                  )}
                  <span>
                    <span>实际导出</span>
                    <strong>{parsed.words.length} 个词</strong>
                  </span>
                  <small>短语会保留为整体；含路径字符、超过 64 个字符或纯数字的条目不导出</small>
                </div>
              )}
            </div>
          ) : (
            <>
              <div className={styles.filters}>
                {source === "history" ? (
                  <div role="group" aria-label="时间范围" className={styles.chips}>
                    {RANGES.map(([r, name]) => (
                      <button key={r} type="button" aria-pressed={range === r} onClick={() => setRange(r)}>
                        {name}
                      </button>
                    ))}
                  </div>
                ) : (
                  <label className={styles.inlineField}>
                    标签
                    <select value={tag} onChange={(e) => setTag(e.target.value)}>
                      <option value="">全部生词</option>
                      {tags.map((t) => (
                        <option key={t.name} value={t.name}>
                          {t.name}（{t.count}）
                        </option>
                      ))}
                    </select>
                  </label>
                )}
              </div>
              <div className={styles.selectBar}>
                <label>
                  <input type="checkbox" checked={wordRows.length > 0 && selectedWords.length === wordRows.length} onChange={(e) => setAll(e.target.checked)} />
                  全选
                </label>
                <span>
                  已选 <strong>{selectedWords.length}</strong> / {wordRows.length} 个词
                </span>
                <span className={styles.hintRight}>Shift 点击可连选</span>
              </div>
              {wordRows.length === 0 ? (
                <p className={styles.empty}>{source === "history" ? "这个时间范围里没有查词记录" : "生词本里还没有词"}</p>
              ) : (
                <VirtualList
                  className={styles.list}
                  label="可选的词"
                  items={rows}
                  rowHeight={ROW}
                  keyOf={(r, i) => (r.kind === "head" ? `h${i}` : r.key)}
                  render={(r, index) =>
                    r.kind === "head" ? (
                      <div className={styles.groupHead}>{r.label}</div>
                    ) : (
                      <label className={styles.wordRow} data-off={!isChecked(r) || undefined}>
                        <input
                          type="checkbox"
                          checked={isChecked(r)}
                          onChange={() => undefined}
                          onClick={(e) => {
                            const on = !isChecked(r);
                            if (e.shiftKey && lastClick.current != null) {
                              const [a, b] = [Math.min(lastClick.current, index), Math.max(lastClick.current, index)];
                              for (const x of rows.slice(a, b + 1)) if (x.kind === "word") toggle(x, on);
                            } else toggle(r, on);
                            lastClick.current = index;
                          }}
                        />
                        <span className={styles.word}>{r.word}</span>
                        <span className={styles.brief}>{r.brief}</span>
                        {r.count > 0 && <span className={styles.count}>查 {r.count} 次</span>}
                      </label>
                    )
                  }
                />
              )}
            </>
          )}
        </div>

        <div className={styles.right}>
          <fieldset className={styles.fieldset}>
            <legend>输出</legend>
            <div className={styles.formats}>
              <label data-checked={pdf || undefined}>
                <input type="checkbox" checked={pdf} onChange={(e) => setPdf(e.target.checked)} />
                PDF
              </label>
              <label data-checked={html || undefined}>
                <input type="checkbox" checked={html} onChange={(e) => setHtml(e.target.checked)} />
                HTML
              </label>
            </div>
            <label className={styles.radio}>
              <input type="radio" name="batch-split" checked={!merge} onChange={() => setMerge(false)} />
              <span>
                每个词一个文件<small>另生成 index.html 总目录</small>
              </span>
            </label>
            <label className={styles.radio}>
              <input type="radio" name="batch-split" checked={merge} onChange={() => setMerge(true)} />
              <span>
                合并成一个文件<small>带目录书签；超过 300 词自动分卷</small>
              </span>
            </label>
          </fieldset>
          <div className={styles.grid2}>
            <label className={styles.field}>
              <span>版式</span>
              <select value={layout} onChange={(e) => setLayout(e.target.value as ExportLayout)}>
                <option value="cards">卡片式</option>
                <option value="document">文档式</option>
              </select>
            </label>
            <label className={styles.field}>
              <span>词典</span>
              <select value={offlineOnly ? "offline" : "profile"} onChange={(e) => setOfflineOnly(e.target.value === "offline")}>
                <option value="profile">{profileName ? `${profileName.name}配置组 · ${profileName.dictCount} 本` : "当前配置组"}</option>
                <option value="offline">仅离线词典（最快）</option>
              </select>
            </label>
            <label className={styles.field}>
              <span>样式</span>
              <select value={style} onChange={(e) => setStyle(e.target.value as ExportStyle)}>
                <option value="paperWhite">纸本（白底）</option>
                <option value="followTheme">跟随当前主题</option>
                <option value="inkSaving">黑白省墨</option>
              </select>
            </label>
            <label className={styles.field}>
              <span>纸张</span>
              <select value={paper} disabled={!pdf} onChange={(e) => setPaper(e.target.value as Paper)}>
                <option value="a4">A4</option>
                <option value="letter">Letter</option>
              </select>
            </label>
          </div>
          <label className={styles.field}>
            <span>保存到</span>
            <span className={styles.dirRow}>
              <input type="text" value={outDir} readOnly aria-label="保存目录" />
              <button
                type="button"
                className={styles.ghost}
                onClick={async () => {
                  const dir = await openDialog({ directory: true, title: "选择保存目录", defaultPath: baseDir ?? undefined });
                  if (typeof dir === "string") setCustomDir(dir);
                }}
              >
                浏览
              </button>
            </span>
          </label>

          <div className={styles.preflight} aria-live="polite">
            <strong>导出前检查</strong>
            <span>
              <span>去重后</span>
              <span>
                {finalWords.length} 个词{mergedCount > 0 && `（合并 ${mergedCount} 个）`}
              </span>
            </span>
            {preflight && (
              <>
                <span>
                  <span>预计用时</span>
                  <span>{fmtDuration(preflight.estSeconds)}</span>
                </span>
                <span>
                  <span>预计大小</span>
                  <span data-warn={!preflight.spaceOk || undefined}>
                    约 {fmtBytes(preflight.estBytes)} · {preflight.freeBytes == null ? "剩余空间未知" : preflight.spaceOk ? "磁盘剩余充足" : `磁盘只剩 ${fmtBytes(preflight.freeBytes)}`}
                  </span>
                </span>
                {preflight.onlineDicts > 0 && (
                  <span>
                    <span>在线词典</span>
                    <span>
                      {preflight.onlineCached} 个已缓存，{preflight.onlineUncached} 个需联网
                    </span>
                  </span>
                )}
                {!preflight.writable && <span className={styles.warn}>{preflight.writeError ?? "目录不可写"}</span>}
              </>
            )}
          </div>
        </div>
      </div>

      <div className={styles.foot}>
        {error ? (
          <span className={styles.error} role="alert">
            {error}
          </span>
        ) : (
          <span className={styles.footHint}>导出在后台进行，期间可以继续查词；中途关闭软件，下次打开可以接着导出</span>
        )}
        <button type="button" className={styles.ghostBtn} onClick={onClose}>
          取消
        </button>
        <button type="button" className={styles.primary} disabled={!canStart} onClick={() => void start()}>
          {busy ? "正在开始…" : `开始导出 ${finalWords.length} 个词`}
        </button>
      </div>
    </>
  );
}
