// 语境句：高亮所查的词；可向前/向后多带几句（弹窗 Alt+← / Alt+→）。
// 偏移是 UTF-16 下标，JS 字符串可直接 slice。

import type { ContextSentence as Ctx } from "../ipc/capture";
import styles from "./ContextSentence.module.css";

interface Props {
  ctx: Ctx;
  /** 向前、向后多带的句数 */
  extraBefore: number;
  extraAfter: number;
}

/** 拼出扩展后的句子和新的高亮区间 */
export function extend(ctx: Ctx, extraBefore: number, extraAfter: number) {
  const pre = ctx.before.slice(ctx.before.length - Math.min(extraBefore, ctx.before.length));
  const post = ctx.after.slice(0, Math.min(extraAfter, ctx.after.length));
  const head = pre.length ? `${pre.join(" ")} ` : "";
  const tail = post.length ? ` ${post.join(" ")}` : "";
  return {
    text: head + ctx.sentence + tail,
    hlStart: head.length + ctx.hlStart,
    hlEnd: head.length + ctx.hlEnd,
  };
}

export function ContextSentence({ ctx, extraBefore, extraAfter }: Props) {
  const { text, hlStart, hlEnd } = extend(ctx, extraBefore, extraAfter);
  return (
    <p className={styles.sentence}>
      {text.slice(0, hlStart)}
      <mark className={styles.mark}>{text.slice(hlStart, hlEnd)}</mark>
      {text.slice(hlEnd)}
    </p>
  );
}
