// 右侧栏（三栏布局）：COCA 词频与搭配、vocabulary.com 真实例句、AI 双语例句。
// COCA 来自用户导入的 2 万词频表；例句由 Claude 生成，按（词, 模型）缓存，查过的词再看不花钱。

import { useEffect, useState } from "react";
import { tr } from "../i18n";
import { aiExamples, aiKeyStatus, cocaLookup, cocaStatus, expandCollocate, type CocaEntry, type Examples } from "../ipc/ai";
import { useRoute } from "../stores/route";
import { useSettings } from "../stores/settings";
import { scrollRoot } from "../windows/main/scroll";
import { VocabExamples } from "./VocabExamples";
import styles from "./WordInsights.module.css";

const POS: Record<string, string> = {
  n: "名词",
  v: "动词",
  j: "形容词",
  r: "副词",
  i: "介词",
  c: "连词",
  d: "限定词",
  p: "代词",
  u: "感叹词",
  m: "数词",
  a: "冠词",
  x: "否定词",
  e: "存在词",
  t: "不定式标记",
};
const GROUP: Record<string, string> = { adj: "形容词", noun: "名词", verb: "动词", adv: "副词", misc: "其他", all: "搭配" };
const REGISTER: Record<string, string> = { S: "口语", F: "小说", M: "杂志", N: "报纸", A: "学术" };

function openAiSettings() {
  useRoute.getState().go("settings");
  setTimeout(() => {
    document.getElementById("set-ai")?.scrollIntoView({ block: "start" });
    if (!document.getElementById("set-ai")) scrollRoot().scrollTo(0, 0);
  }, 50);
}

function CocaSection({ word }: { word: string }) {
  const [entries, setEntries] = useState<CocaEntry[] | null>(null);
  const [loaded, setLoaded] = useState<boolean | null>(null);

  useEffect(() => {
    let alive = true;
    void cocaStatus().then((s) => alive && setLoaded(s.loaded), () => alive && setLoaded(false));
    void cocaLookup(word).then((e) => alive && setEntries(e), () => alive && setEntries([]));
    return () => {
      alive = false;
    };
  }, [word]);

  if (loaded === false) {
    return (
      <section className={styles.section}>
        <div className={styles.label}>{tr("COCA 词频与搭配")}</div>
        <p className={styles.hint}>
          {tr("还没有导入 COCA 2 万词频表。")}{" "}
          <button type="button" className={styles.link} onClick={openAiSettings}>
            {tr("去导入")}
          </button>
        </p>
      </section>
    );
  }
  if (!entries) return null;
  if (entries.length === 0) {
    return (
      <section className={styles.section}>
        <div className={styles.label}>{tr("COCA 词频与搭配")}</div>
        <p className={styles.hint}>{tr("不在 COCA 前 2 万词里（较少用的词或短语）。")}</p>
      </section>
    );
  }
  return (
    <section className={styles.section} aria-label={tr("COCA 词频与搭配")} data-word-lookup="">
      <div className={styles.label}>{tr("COCA 词频与搭配")}</div>
      {entries.map((e) => (
        <div key={`${e.rank}-${e.pos}`} className={styles.entry}>
          <div className={styles.head}>
            <strong className={styles.rank}>#{e.rank.toLocaleString()}</strong>
            <span className={styles.pos}>{tr(POS[e.pos] ?? e.pos)}</span>
            <span className={styles.meta}>{tr("频次 {n}", { n: e.freq.toLocaleString() })}</span>
            {e.register && (
              <span className={styles.register} title={tr("在这类文本里明显更常见")}>
                {e.register
                  .split(" ")
                  .map((r) => tr(REGISTER[r] ?? r))
                  .join(" · ")}
              </span>
            )}
          </div>
          {Object.entries(e.collocates).map(([group, list]) => {
            const items = list.split(",").map((s) => s.trim()).filter(Boolean);
            if (items.length === 0) return null;
            return (
              <div key={group} className={styles.group}>
                <span className={styles.groupName}>{tr(GROUP[group] ?? group)}</span>
                <span className={styles.chips}>
                  {items.map((c) => (
                    <span key={c} className={styles.chip}>
                      {expandCollocate(e.lemma, c)}
                    </span>
                  ))}
                </span>
              </div>
            );
          })}
          {e.synonyms.length > 0 && (
            <div className={styles.group}>
              <span className={styles.groupName}>{tr("近义")}</span>
              <span className={styles.synonyms}>{e.synonyms.join(" ‖ ")}</span>
            </div>
          )}
        </div>
      ))}
    </section>
  );
}

function AiSection({ word, brief }: { word: string; brief: string | null }) {
  const auto = useSettings((s) => s.settings?.ai?.auto ?? false);
  const [hasKey, setHasKey] = useState<boolean | null>(null);
  const [data, setData] = useState<Examples | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const run = async (force: boolean, cachedOnly: boolean) => {
    setError(null);
    if (!cachedOnly) setBusy(true);
    try {
      const r = await aiExamples(word, { brief, force, cachedOnly });
      setData(r);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    let alive = true;
    setData(null);
    setError(null);
    void aiKeyStatus().then(
      (k) => {
        if (!alive) return;
        setHasKey(k);
        // 有缓存总是直接显示；开了「自动生成」且有 Key 时才会调用接口
        void run(false, !(k && auto));
      },
      () => alive && setHasKey(false),
    );
    return () => {
      alive = false;
    };
  }, [word, auto]);

  return (
    <section className={styles.section} aria-label={tr("AI 双语例句")} data-word-lookup="">
      <div className={styles.labelRow}>
        <span className={styles.label}>{tr("AI 双语例句")}</span>
        {hasKey && data && (
          <button type="button" className={styles.link} disabled={busy} onClick={() => void run(true, false)}>
            {tr("重新生成")}
          </button>
        )}
      </div>
      {hasKey === false && !data && (
        <p className={styles.hint}>
          {tr("填写 Claude API Key 后，可以按 COCA 真实搭配生成中英双语例句。")}{" "}
          <button type="button" className={styles.link} onClick={openAiSettings}>
            {tr("去设置")}
          </button>
        </p>
      )}
      {hasKey && !data && !busy && (
        <button type="button" className={styles.generate} onClick={() => void run(false, false)}>
          {tr("生成双语例句")}
        </button>
      )}
      {busy && <p className={styles.hint} aria-live="polite">{tr("正在生成…")}</p>}
      {error && (
        <p className={styles.error} role="alert">
          {error}
        </p>
      )}
      {data && data.examples.length > 0 && (
        <ol className={styles.examples}>
          {data.examples.map((x, i) => (
            <li key={i}>
              <p className={styles.en}>{x.en}</p>
              <p className={styles.zh}>{x.zh}</p>
              {x.collocation && <span className={styles.chip}>{x.collocation}</span>}
            </li>
          ))}
        </ol>
      )}
      {data && <p className={styles.foot}>{data.cachedAt ? tr("{model} · 已缓存", { model: data.model }) : data.model}</p>}
    </section>
  );
}

export function WordInsights({ word, brief }: { word: string; brief: string | null }) {
  return (
    <>
      <CocaSection word={word} />
      <VocabExamples word={word} variant="panel" />
      <AiSection word={word} brief={brief} />
    </>
  );
}
