// 英文怎么说：搜索框里是中文意思（或一整句英文）时出现在查词结果最上面。
// 折叠着不花钱；点开才用 DeepSeek 生成五种说法（有缓存直接显示）。英文里的词点了就查。

import { useEffect } from "react";
import { tr } from "../i18n";
import { expressionMode, tooLong } from "../expression/detect";
import { useExpression } from "../stores/expression";
import { useSettings } from "../stores/settings";
import { useRoute } from "../stores/route";
import { toast } from "../stores/toast";
import { isoMinute } from "../export-templates/template";
import { splitWords } from "./sentence";
import { CopyIcon } from "./CopyMenu";
import styles from "./ExpressionCard.module.css";

const EDIT = "__edit__";

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
      useRoute.getState().go("settings");
      setTimeout(() => document.getElementById("set-expression")?.scrollIntoView({ block: "start" }), 200);
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
          <footer className={styles.foot}>
            <span>
              DeepSeek · {result.cachedAt !== null ? `${tr("已存本机，再看不花钱")} · ${isoMinute(result.cachedAt)}` : fen(result.costUsd)}
            </span>
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
            onClick={() => {
              useRoute.getState().go("settings");
              setTimeout(() => document.getElementById("set-ai")?.scrollIntoView({ block: "start" }), 200);
            }}
          >
            {tr("去填写")}
          </button>
        </p>
      )}
    </section>
  );
}
