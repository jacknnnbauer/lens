// 英文怎么说：搜索框里是中文意思（或一整句英文）时出现在查词结果最上面。
// 折叠着不花钱；点开才用 DeepSeek 生成五种说法（有缓存直接显示）。英文里的词点了就查。

import { useEffect } from "react";
import { tr } from "../i18n";
import { expressionMode, isShortPhrase, tooLong } from "../expression/detect";
import type { EntryResult } from "../ipc/expression";
import { useExpression } from "../stores/expression";
import { useSettings } from "../stores/settings";
import { useRoute } from "../stores/route";
import { toast } from "../stores/toast";
import { isoMinute } from "../export-templates/template";
import { splitWords } from "./sentence";
import { CopyIcon } from "./CopyMenu";
import styles from "./ExpressionCard.module.css";

const EDIT = "__edit__";

/** 打开设置并滚到某一节（设置页按需加载，等它出现再滚） */
function openSettingsAt(id: string) {
  useRoute.getState().go("settings");
  let n = 0;
  const t = setInterval(() => {
    const el = document.getElementById(id);
    if (el || ++n > 40) {
      clearInterval(t);
      el?.scrollIntoView({ block: "start" });
    }
  }, 100);
}

// 和长难句拆解一样按 1 美元 ≈ 7.2 元折算
const fen = (usd: number) => (usd * 7.2 * 100 < 0.1 ? tr("不到 0.1 分") : tr("{n} 分", { n: (usd * 7.2 * 100).toFixed(1) }));

/** 把一句英文切成可点的词，词之间的空格、标点原样保留 */
function ClickableEnglish({ text, onLookup }: { text: string; onLookup: (w: string) => void }) {
  const words = splitWords(text);
  const out: React.ReactNode[] = [];
  let pos = 0;
  words.forEach((w, i) => {
    if (w.start > pos) out.push(text.slice(pos, w.start));
    out.push(
      <button key={i} type="button" className={styles.word} onClick={() => onLookup(w.text)}>
        {w.text}
      </button>,
    );
    pos = w.end;
  });
  if (pos < text.length) out.push(text.slice(pos));
  return <>{out}</>;
}

// 和长难句拆解一样的费用说明
const footText = (r: { cachedAt: number | null; costUsd: number }) =>
  `DeepSeek · ${r.cachedAt !== null ? `${tr("已存本机，再看不花钱")} · ${isoMinute(r.cachedAt)}` : fen(r.costUsd)}`;

/** 「更多说法」：短的中文说法在五种风格下面可以展开分组词条 */
function EntrySection({ text, onLookup, onCopy }: { text: string; onLookup: (w: string) => void; onCopy: (en: string) => void }) {
  const { entryOpen, entryStatus, entryResult, entryError, openEntry, closeEntry } = useExpression();
  if (!entryOpen) {
    return (
      <button type="button" className={styles.more} onClick={() => void openEntry(text)}>
        {tr("更多说法")} ›
      </button>
    );
  }
  return (
    <div className={styles.entry}>
      <button type="button" className={styles.more} aria-expanded="true" onClick={closeEntry}>
        {tr("更多说法")} ⌄
      </button>
      {entryStatus === "loading" && (
        <div aria-busy="true" className={styles.list}>
          {[0, 1, 2].map((i) => (
            <div key={i} className={styles.skeleton} />
          ))}
        </div>
      )}
      {entryStatus === "done" && entryResult && <EntryBody result={entryResult} onLookup={onLookup} onCopy={onCopy} onRedo={() => void openEntry(text, true)} />}
      {entryStatus === "error" && (
        <p className={styles.error}>
          {entryError}{" "}
          <button type="button" className={styles.link} onClick={() => void openEntry(text)}>
            {tr("重试")}
          </button>
        </p>
      )}
      {entryStatus === "nokey" && (
        <p className={styles.error}>
          {tr("要先填写 DeepSeek API Key。")}{" "}
          <button type="button" className={styles.link} onClick={() => openSettingsAt("set-ai")}>
            {tr("去填写")}
          </button>
        </p>
      )}
    </div>
  );
}

function EntryBody({ result, onLookup, onCopy, onRedo }: { result: EntryResult; onLookup: (w: string) => void; onCopy: (en: string) => void; onRedo: () => void }) {
  const { groups, dialogue, tip } = result.entry;
  return (
    <>
      {groups.map((g, gi) => (
        <section key={gi} className={styles.group} aria-label={g.name}>
          <h4 className={styles.groupName}>{g.name}</h4>
          <ul className={styles.entryList}>
            {g.items.map((it, i) => (
              <li key={i} className={styles.entryItem}>
                <p className={styles.en}>
                  <ClickableEnglish text={it.en} onLookup={onLookup} />
                  {it.star && (
                    <span className={styles.star} title={tr("常用")}>
                      ★
                    </span>
                  )}
                  {it.tag && <span className={styles.tag}>{tr(it.tag)}</span>}
                </p>
                <button type="button" className={styles.copy} aria-label={tr("复制这一句")} title={tr("复制这一句")} onClick={() => onCopy(it.en)}>
                  <CopyIcon />
                </button>
                {(it.zh || it.when) && (
                  <p className={styles.note}>
                    {it.zh}
                    {it.when && ` · ${it.when}`}
                  </p>
                )}
              </li>
            ))}
          </ul>
        </section>
      ))}
      {dialogue.length > 0 && (
        <section className={styles.group} aria-label={tr("对话")}>
          <h4 className={styles.groupName}>{tr("对话")}</h4>
          <div className={styles.dialogue}>
            {dialogue.map((l, i) => (
              <div key={i}>
                <p className={styles.en}>
                  {l.who && <span className={styles.who}>{l.who}:</span>} <ClickableEnglish text={l.en} onLookup={onLookup} />
                </p>
                {l.zh && <p className={styles.dialogueZh}>{l.zh}</p>}
              </div>
            ))}
          </div>
        </section>
      )}
      {tip && <p className={styles.tip}>{tr("记整块：{tip}", { tip })}</p>}
      <footer className={styles.foot}>
        <span>{footText(result)}</span>
        <button type="button" className={styles.link} onClick={onRedo}>
          {tr("换一批")}
        </button>
      </footer>
    </>
  );
}

export function ExpressionCard({ input, onLookup }: { input: string; onLookup: (w: string) => void }) {
  const mode = expressionMode(input);
  const audiences = useSettings((s) => s.settings?.expression?.audiences) ?? [];
  const lastId = useSettings((s) => s.settings?.expression?.lastAudience) ?? "";
  const update = useSettings((s) => s.update);
  // 上次选的对象被删了：当作不指定
  const audience = audiences.find((a) => a.id === lastId) ?? null;
  const note = audience ? `${audience.name}：${audience.note}` : null;
  const { status, result, error, show, run } = useExpression();
  const text = input.trim();
  const long = tooLong(text);

  // 输入或对象变了：有缓存就直接显示，没有就折叠着
  useEffect(() => {
    if (mode && !long) void show(text, note);
  }, [text, note, mode, long, show]);

  if (!mode) return null;

  const pickAudience = (v: string) => {
    if (v === EDIT) {
      openSettingsAt("set-expression");
      return;
    }
    update({ expression: { lastAudience: v } });
    // 已经展开着：用新对象再要一次（有缓存不花钱）
    if (status !== "idle") {
      const a = audiences.find((x) => x.id === v);
      void run(text, a ? `${a.name}：${a.note}` : null);
    }
  };

  const copy = async (en: string) => {
    try {
      await navigator.clipboard.writeText(en);
      toast(tr("已复制"));
    } catch (e) {
      toast(String(e));
    }
  };

  const open = !long && status === "idle" ? () => void run(text, note) : undefined;

  return (
    <section className={styles.card} aria-label={tr(mode === "zh" ? "英文怎么说" : "换个说法")} data-ui="card">
      <header className={styles.head}>
        <button type="button" className={styles.title} onClick={open} disabled={!open} aria-expanded={status !== "idle"}>
          {tr(mode === "zh" ? "英文怎么说" : "换个说法")}
          {status === "idle" && <span className={styles.sub}>{long ? tr("太长了，挑一两句再试") : tr("点开，用 DeepSeek 给出五种说法")}</span>}
        </button>
        <select className={styles.audience} value={audience?.id ?? ""} onChange={(e) => pickAudience(e.target.value)} aria-label={tr("对象")}>
          <option value="">{tr("不指定对象")}</option>
          {audiences.map((a) => (
            <option key={a.id} value={a.id}>
              {a.name}
            </option>
          ))}
          <option value={EDIT}>{tr("编辑对象…")}</option>
        </select>
      </header>

      {status === "loading" && (
        <ol className={styles.list} aria-busy="true">
          {[0, 1, 2, 3, 4].map((i) => (
            <li key={i} className={styles.skeleton} />
          ))}
        </ol>
      )}

      {status === "done" && result && (
        <>
          <ol className={styles.list}>
            {result.expression.items.map((it) => (
              <li key={it.style} className={styles.item}>
                <span className={styles.style}>{tr(it.style)}</span>
                <p className={styles.en}>
                  <ClickableEnglish text={it.en} onLookup={onLookup} />
                </p>
                <button type="button" className={styles.copy} aria-label={tr("复制这一句")} title={tr("复制这一句")} onClick={() => void copy(it.en)}>
                  <CopyIcon />
                </button>
                {it.note && <p className={styles.note}>{it.note}</p>}
              </li>
            ))}
          </ol>
          {mode === "zh" && isShortPhrase(text) && <EntrySection text={text} onLookup={onLookup} onCopy={(en) => void copy(en)} />}
          <footer className={styles.foot}>
            <span>{footText(result)}</span>
            <button type="button" className={styles.link} onClick={() => void run(text, note, true)}>
              {tr("换一批")}
            </button>
          </footer>
        </>
      )}

      {status === "error" && (
        <p className={styles.error}>
          {error}{" "}
          <button type="button" className={styles.link} onClick={() => void run(text, note)}>
            {tr("重试")}
          </button>
        </p>
      )}

      {status === "nokey" && (
        <p className={styles.error}>
          {tr("要先填写 DeepSeek API Key。")}{" "}
          <button
            type="button"
            className={styles.link}
            onClick={() => openSettingsAt("set-ai")}
          >
            {tr("去填写")}
          </button>
        </p>
      )}
    </section>
  );
}
