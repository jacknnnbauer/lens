// vocabulary.com 词典卡片底部的「真实例句」：原词条里由词典自带脚本在线加载，Lens 不运行词典脚本，
// 改由 Rust 取 corpus.vocabulary.com 的例句（缓存，离线时用缓存）。

import { openUrl } from "@tauri-apps/plugin-opener";
import { useEffect, useState } from "react";
import { tr } from "../i18n";
import { vocabExamples, type VocabPage, type VocabSentence } from "../ipc/vocab";
import styles from "./VocabExamples.module.css";

const DOMAINS: Array<[string, string]> = [
  ["", "全部来源"],
  ["F", "小说"],
  ["A", "艺术文化"],
  ["N", "新闻"],
  ["B", "商业"],
  ["S", "体育"],
  ["M", "科学医学"],
  ["T", "科技"],
];

/** 词条 HTML 是不是 vocabulary.com 的（按页面结构认，不依赖词典名） */
export function isVocabularyCom(html: string): boolean {
  return html.includes('id="voc_main"') || html.includes('class="wordPage');
}

function Highlighted({ s }: { s: VocabSentence }) {
  if (!s.hl) return <>{s.text}</>;
  // 位置按字符（码点）计
  const chars = Array.from(s.text);
  const [a, b] = s.hl;
  return (
    <>
      {chars.slice(0, a).join("")}
      <mark>{chars.slice(a, b).join("")}</mark>
      {chars.slice(b).join("")}
    </>
  );
}

function year(ms: number | null): string {
  return ms ? String(new Date(ms).getFullYear()) : "";
}

export function VocabExamples({ word }: { word: string }) {
  const [domain, setDomain] = useState("");
  const [pages, setPages] = useState<VocabPage[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [open, setOpen] = useState(true);

  const load = async (d: string, offset: number) => {
    setBusy(true);
    setError(null);
    try {
      const p = await vocabExamples(word, { domain: d, offset });
      setPages((prev) => (offset === 0 ? [p] : [...prev, p]));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    setPages([]);
    void load(domain, 0);
  }, [word, domain]);

  const sentences = pages.flatMap((p) => p.sentences);
  const total = pages[0]?.total ?? 0;
  const last = pages[pages.length - 1];
  const more = last && last.offset + last.sentences.length < total && last.sentences.length > 0;
  const stale = pages.some((p) => p.stale);

  return (
    <section className={styles.box} aria-label={tr("真实例句")}>
      <header className={styles.head}>
        <button type="button" className={styles.title} aria-expanded={open} onClick={() => setOpen((o) => !o)}>
          {tr("真实例句")}
          {total > 0 && <small>{tr("{n} 句", { n: total.toLocaleString() })}</small>}
        </button>
        <span className={styles.via}>vocabulary.com</span>
      </header>
      {open && (
        <>
          <div className={styles.domains} role="group" aria-label={tr("例句来源")}>
            {DOMAINS.map(([code, name]) => (
              <button key={code || "all"} type="button" aria-pressed={domain === code} onClick={() => setDomain(code)}>
                {tr(name)}
              </button>
            ))}
          </div>
          {stale && <p className={styles.note}>{tr("离线：显示的是之前缓存的例句")}</p>}
          {error && sentences.length === 0 && (
            <p className={styles.note} role="alert">
              {tr("例句加载失败：{error}", { error })}
            </p>
          )}
          {!busy && !error && sentences.length === 0 && <p className={styles.note}>{tr("这个来源下没有例句")}</p>}
          <ul className={styles.list}>
            {sentences.map((s, i) => (
              <li key={i}>
                <p className={styles.sentence}>
                  <Highlighted s={s} />
                </p>
                <p className={styles.source}>
                  {s.url ? (
                    <a
                      href={s.url}
                      onClick={(e) => {
                        e.preventDefault();
                        void openUrl(s.url ?? "");
                      }}
                    >
                      {s.title ?? s.source}
                    </a>
                  ) : (
                    <span>{s.title ?? s.source}</span>
                  )}
                  {s.title && s.source && <span> · {s.source}</span>}
                  {s.published && <span> · {year(s.published)}</span>}
                </p>
              </li>
            ))}
          </ul>
          {busy && <p className={styles.note}>{tr("正在加载…")}</p>}
          {more && !busy && (
            <button type="button" className={styles.more} onClick={() => void load(domain, last.offset + last.sentences.length)}>
              {tr("更多例句")}
            </button>
          )}
        </>
      )}
    </section>
  );
}
