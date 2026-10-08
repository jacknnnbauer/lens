// 批量导出 · 进度（画板 11）：总进度、三个阶段、内存、逐词状态（虚拟滚动）、暂停 / 取消 / 重试。

import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { useEffect, useMemo, useRef, useState } from "react";
import type { BatchItem } from "../../ipc/batch";
import { useBatch } from "../../stores/batch";
import { useSettings } from "../../stores/settings";
import styles from "./Batch.module.css";
import { VirtualList } from "./VirtualList";
import { tr } from "../../i18n";

const ROW = 34;
type Filter = "all" | "active" | "noted" | "failed";

function clock(ms: number): string {
  const s = Math.max(0, Math.round(ms / 1000));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = String(s % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${sec}` : `${m}:${sec}`;
}

const isActive = (i: BatchItem) => i.state === "querying" || i.state === "rendering";
const isNoted = (i: BatchItem) => i.state === "done" && !!i.note;

function ItemRow({ item }: { item: BatchItem }) {
  const icon =
    item.state === "done" ? (item.note ? "!" : "✓") : item.state === "failed" ? "✕" : isActive(item) ? "" : "·";
  const text =
    item.state === "done"
      ? item.note
        ? `${item.files.join(" · ")} — ${item.note}`
        : item.files.join(" · ")
      : item.state === "failed"
        ? (item.note ?? tr("失败"))
        : item.state === "querying"
          ? tr("查词中")
          : item.state === "rendering"
            ? tr("渲染中")
            : tr("等待");
  return (
    <div className={styles.itemRow} data-state={item.state} data-noted={isNoted(item) || undefined}>
      <span className={styles.icon} aria-hidden="true">
        {isActive(item) ? <span className={styles.spinner} /> : icon}
      </span>
      <span className={styles.word}>{item.lemma}</span>
      <span className={styles.itemText} title={text}>
        {text}
      </span>
    </div>
  );
}

export function BatchProgress({ onClose }: { onClose: () => void }) {
  const current = useBatch((s) => s.current);
  const items = useBatch((s) => s.items);
  const live = useBatch((s) => s.live);
  const error = useBatch((s) => s.error);
  const { pause, resume, cancel, retryFailed, backToSelect } = useBatch.getState();
  const concurrency = useSettings((s) => s.settings?.batch.lookupConcurrency) ?? 4;
  const [filter, setFilter] = useState<Filter>("all");
  const [now, setNow] = useState(Date.now());
  const [confirmCancel, setConfirmCancel] = useState(false);
  // 本轮开始时已处理的数量，用来估算剩余时间
  const baseline = useRef<{ processed: number; at: number } | null>(null);

  const counts = live?.counts ?? current?.counts ?? null;
  const state = live?.state ?? current?.state ?? "running";
  const running = state === "running" || state === "paused";
  const processed = counts ? counts.done + counts.failed : 0;

  useEffect(() => {
    if (!running) return;
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, [running]);

  useEffect(() => {
    if (state === "running" && counts && !baseline.current) baseline.current = { processed, at: Date.now() };
    if (state !== "running") baseline.current = null;
  }, [state, counts, processed]);

  const shown = useMemo(() => {
    if (filter === "active") return items.filter(isActive);
    if (filter === "noted") return items.filter(isNoted);
    if (filter === "failed") return items.filter((i) => i.state === "failed");
    return items;
  }, [items, filter]);

  if (!current || !counts) return null;
  const total = counts.total || 1;
  const pct = Math.round((processed / total) * 100);
  const elapsed = live?.startedAt ? now - live.startedAt : 0;
  const b = baseline.current;
  const rate = b && processed > b.processed ? (now - b.at) / (processed - b.processed) : null;
  const remaining = rate ? rate * (total - processed) : null;
  const formats = [current.pdf && "PDF", current.html && "HTML"].filter(Boolean).join(" + ");
  const reveal = () => void revealItemInDir(state === "done" || state === "cancelled" ? `${current.outDir}\\index.html` : current.outDir);

  const stateText =
    state === "paused" ? tr("已暂停") : state === "done" ? tr("已完成") : state === "cancelled" ? tr("已取消") : state === "error" ? tr("出错，已暂停") : null;

  return (
    <>
      <div className={styles.head}>
        <h2 id="batch-title">{tr("批量导出")}</h2>
        <span className={styles.headMeta}>
          {current.label} · {tr("{n} 个词", { n: counts.total })} · {formats} · {tr(current.layout === "document" ? "文档式" : "卡片式")}
          {current.merge && ` · ${tr("合并")}`}
        </span>
        <button type="button" className={styles.ghost} onClick={onClose}>
          {tr(running ? "转到后台" : "关闭")}
        </button>
      </div>

      <div className={styles.progressBody}>
        <div className={styles.summary}>
          <div className={styles.bigRow}>
            <span className={styles.big}>
              {processed} / {counts.total}
            </span>
            <span className={styles.sub}>
              {stateText ? `${stateText} · ` : ""}
              {elapsed > 0 && tr("已用 {t}", { t: clock(elapsed) })}
              {remaining != null && state === "running" && ` · ${tr("剩余约 {t}", { t: clock(remaining) })}`}
            </span>
          </div>
          <div role="progressbar" aria-valuenow={pct} aria-valuemin={0} aria-valuemax={100} aria-label={tr("总进度")} className={styles.bar}>
            <div style={{ width: `${pct}%` }} data-state={state} />
          </div>
        </div>

        <div className={styles.stages}>
          <div>
            <span>{tr("① 查词")}</span>
            <strong>
              {tr("{c} 并发 · 进行 {n}", { c: concurrency, n: counts.querying })}
            </strong>
            <small>{tr("在线源限速 2 次/秒")}</small>
          </div>
          <div>
            <span>{tr("② 渲染")}</span>
            <strong>
              {tr("{n} 个 · 排队 {q}/8", { n: counts.rendering, q: live?.renderQueue ?? 0 })}
            </strong>
            <small>{tr((live?.renderQueue ?? 0) >= 8 ? "队列满，查词自动放慢" : "导出窗口逐个渲染")}</small>
          </div>
          <div>
            <span>{tr("③ 写入")}</span>
            <strong>{tr("{n} 个文件", { n: counts.files })}</strong>
            <small>{tr("先写临时文件再改名")}</small>
          </div>
        </div>

        <div className={styles.memLine}>
          {live && live.memMb > 0 && (
            <span>
              {tr("内存 {mem} MB / 上限 {limit} MB", { mem: live.memMb, limit: live.memLimitMb })}
            </span>
          )}
          {live && live.recycles > 0 && <span>{tr("渲染窗口已重建 {n} 次", { n: live.recycles })}</span>}
          <span>{tr("进度已保存，可随时中断")}</span>
          <button type="button" className={styles.link} onClick={reveal}>
            {tr("打开文件夹")}
          </button>
        </div>

        <div className={styles.itemsBox}>
          <div role="group" aria-label={tr("筛选")} className={styles.chips}>
            {(
              [
                ["all", "全部", counts.total],
                ["active", "进行中", counts.querying + counts.rendering],
                ["noted", "有提示", counts.noted],
                ["failed", "失败", counts.failed],
              ] as const
            ).map(([f, name, n]) => (
              <button key={f} type="button" aria-pressed={filter === f} onClick={() => setFilter(f)}>
                {tr(name)}
                {f !== "all" && ` ${n}`}
              </button>
            ))}
          </div>
          <VirtualList className={styles.itemList} label={tr("逐词状态")} items={shown} rowHeight={ROW} keyOf={(i) => i.seq} render={(i) => <ItemRow item={i} />} />
        </div>
      </div>

      <div className={styles.foot}>
        {error || live?.error ? (
          <span className={styles.error} role="alert">
            {error ?? live?.error}
          </span>
        ) : (
          <span className={styles.footHint}>{tr("取消时已导出的文件会保留；完成后会生成 index.html 总目录和失败清单")}</span>
        )}
        {running ? (
          <>
            {confirmCancel ? (
              <>
                <button type="button" className={styles.danger} onClick={() => void cancel().then(() => setConfirmCancel(false))}>
                  {tr("确认取消")}
                </button>
                <button type="button" className={styles.ghostBtn} onClick={() => setConfirmCancel(false)}>
                  {tr("不取消")}
                </button>
              </>
            ) : (
              <button type="button" className={styles.ghostBtn} onClick={() => setConfirmCancel(true)}>
                {tr("取消")}
              </button>
            )}
            {state === "paused" ? (
              <button type="button" className={styles.primary} onClick={() => void resume()}>
                {tr("继续")}
              </button>
            ) : (
              <button type="button" className={styles.primary} onClick={() => void pause()}>
                {tr("暂停")}
              </button>
            )}
          </>
        ) : (
          <>
            {counts.failed > 0 && state !== "error" && (
              <button type="button" className={styles.ghostBtn} onClick={() => void retryFailed()}>
                {tr("重试失败项")}
              </button>
            )}
            {state === "error" && (
              <button type="button" className={styles.ghostBtn} onClick={() => void resume()}>
                {tr("继续")}
              </button>
            )}
            <button type="button" className={styles.ghostBtn} onClick={reveal}>
              {tr("打开文件夹")}
            </button>
            <button
              type="button"
              className={styles.primary}
              onClick={() => {
                backToSelect();
                onClose();
              }}
            >
              {tr("完成")}
            </button>
          </>
        )}
      </div>
    </>
  );
}
