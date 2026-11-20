// 右栏「拆解」页：本次阅读拆过的长难句。展开的那条显示原句（主干加粗、各成分彩色下划线）、
// 主干成分、各从句和短语、译文、难点；底部是模型、费用和「重新拆解」。

import { Fragment } from "react";
import { tr } from "../../i18n";
import type { Breakdown } from "../../ipc/sentence";
import { placePieces } from "../../sentence/place";
import { useRoute } from "../../stores/route";
import { useSentences, type SentenceItem } from "../../stores/sentences";
import styles from "./SentencePanel.module.css";

/** 原句分段：每段标上属于主干还是第几个成分 */
function segments(sentence: string, b: Breakdown): { text: string; skeleton: boolean; part: number | null }[] {
  const n = sentence.length;
  const skel = new Array<boolean>(n).fill(false);
  const part = new Array<number | null>(n).fill(null);
  placePieces(sentence, b.skeleton.map((p) => p.text)).forEach((r) => {
    if (r) for (let i = r[0]; i < r[1]; i++) skel[i] = true;
  });
  // 先放长的成分，短的（嵌在里面的）后放、覆盖在上面
  const placed = placePieces(sentence, b.parts.map((p) => p.text)).map((r, i) => ({ r, i }));
  for (const { r, i } of placed.filter((x) => x.r).sort((x, y) => y.r![1] - y.r![0] - (x.r![1] - x.r![0]))) for (let k = r![0]; k < r![1]; k++) part[k] = i;
  const out: { text: string; skeleton: boolean; part: number | null }[] = [];
  for (let i = 0; i < n; i++) {
    const last = out[out.length - 1];
    if (last && last.skeleton === skel[i] && last.part === part[i]) last.text += sentence[i];
    else out.push({ text: sentence[i], skeleton: skel[i], part: part[i] });
  }
  return out;
}

const fen = (usd: number) => (usd * 7.2 * 100 < 0.1 ? tr("不到 0.1 分") : tr("{n} 分", { n: (usd * 7.2 * 100).toFixed(1) }));

function Result({ item }: { item: SentenceItem }) {
  const r = item.result!;
  const b = r.breakdown;
  const run = useSentences((s) => s.run);
  return (
    <div className={styles.result}>
      <p className={styles.sentence}>
        {segments(item.sentence, b).map((s, i) => (
          <span key={i} className={s.skeleton ? styles.skel : undefined} data-part={s.part === null ? undefined : s.part % 6}>
            {s.text}
          </span>
        ))}
      </p>
      {b.skeleton.length > 0 && (
        <div className={styles.block}>
          <h4>{tr("主干")}</h4>
          <div className={styles.skeleton}>
            {b.skeleton.map((p, i) => (
              <span key={i}>
                <b>{p.text}</b>
                <small>{p.role}</small>
              </span>
            ))}
          </div>
        </div>
      )}
      {b.parts.length > 0 && (
        <ol className={styles.parts}>
          {b.parts.map((p, i) => (
            <li key={i} data-part={i % 6}>
              <q>{p.text}</q>
              <span className={styles.kind}>
                {p.kind}
                {p.modifies && (
                  <>
                    {" → "}
                    {tr("修饰 {w}", { w: p.modifies })}
                  </>
                )}
              </span>
              {p.zh && <span className={styles.zh}>{p.zh}</span>}
            </li>
          ))}
        </ol>
      )}
      <div className={styles.block}>
        <h4>{tr("译文")}</h4>
        <p className={styles.translation}>{b.translation}</p>
      </div>
      {b.points.length > 0 && (
        <div className={styles.block}>
          <h4>{tr("难点")}</h4>
          {b.points.map((p, i) => (
            <p key={i} className={styles.point}>
              <b>{p.title}</b>
              {p.explain && <>：{p.explain}</>}
            </p>
          ))}
        </div>
      )}
      <div className={styles.foot}>
        <span>
          DeepSeek · {r.cachedAt ? tr("已存本机，再看不花钱") : fen(r.costUsd)}
        </span>
        <button type="button" className={styles.link} onClick={() => void run(item.sentence, item.before, item.after, true)}>
          {tr("重新拆解")}
        </button>
      </div>
    </div>
  );
}

function Body({ item }: { item: SentenceItem }) {
  const run = useSentences((s) => s.run);
  if (item.status === "loading") return <p className={styles.note}>{tr("正在拆解…")}</p>;
  if (item.status === "nokey")
    return (
      <div className={styles.note}>
        <p>{tr("拆解长句要用 DeepSeek，请先在 设置 › AI 服务 里填 Key。")}</p>
        <button
          type="button"
          className={styles.link}
          onClick={() => {
            useRoute.getState().go("settings");
            setTimeout(() => document.getElementById("set-ai")?.scrollIntoView({ block: "start" }), 300);
          }}
        >
          {tr("去填写")}
        </button>
      </div>
    );
  if (item.status === "error")
    return (
      <div className={styles.note}>
        <p className={styles.error}>{item.error}</p>
        <button type="button" className={styles.link} onClick={() => void run(item.sentence, item.before, item.after, true)}>
          {tr("再试一次")}
        </button>
      </div>
    );
  return <Result item={item} />;
}

export function SentencePanel() {
  const items = useSentences((s) => s.items);
  const open = useSentences((s) => s.open);
  const setOpen = useSentences((s) => s.setOpen);
  if (items.length === 0) return <p className={styles.empty}>{tr("选中正文里一句读不懂的长句，点「拆解」，这里会讲清它的结构、译文和难点。")}</p>;
  return (
    <div className={styles.list}>
      {items.map((it) => (
        <Fragment key={it.id}>
          {open === it.id ? (
            <section className={styles.item} data-open="">
              <Body item={it} />
            </section>
          ) : (
            <button type="button" className={styles.item} onClick={() => setOpen(it.id)}>
              <span className={styles.collapsed}>{it.sentence}</span>
            </button>
          )}
        </Fragment>
      ))}
    </div>
  );
}
