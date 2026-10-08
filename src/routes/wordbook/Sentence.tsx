import type { ContextRow } from "../../ipc/history";

/** 语境句，查的词高亮 */
export function Sentence({ ctx }: { ctx: ContextRow }) {
  return (
    <>
      {ctx.sentence.slice(0, ctx.hlStart)}
      <mark>{ctx.sentence.slice(ctx.hlStart, ctx.hlEnd)}</mark>
      {ctx.sentence.slice(ctx.hlEnd)}
    </>
  );
}
