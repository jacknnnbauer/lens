// 批量导出 · 进度（画板 11）：总进度、三个阶段、内存、逐词状态（虚拟滚动）、暂停 / 取消 / 重试。

import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { useEffect, useMemo, useRef, useState } from "react";
import type { BatchItem } from "../../ipc/batch";
import { useBatch } from "../../stores/batch";
import { useSettings } from "../../stores/settings";
import styles from "./Batch.module.css";
import { VirtualList } from "./VirtualList";

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
        ? (item.note ?? "失败")
        : item.state === "querying"
          ? "查词中"
          : item.state === "rendering"
            ? "渲染中"
            : "等待";
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
    state === "paused" ? "已暂停" : state === "done" ? "已完成" : state === "cancelled" ? "已取消" : state === "error" ? "出错，已暂停" : null;

  return (
    <>
      <div className={styles.head}>
        <h2 id="batch-title">批量导出</h2>
        <span className={styles.headMeta}>
          {current.label} · {counts.total} 个词 · {formats} · {current.layout === "document" ? "文档式" : "卡片式"}
          {current.merge && " · 合并"}
        </span>
        <button type="button" className={styles.ghost} onClick={onClose}>
          {running ? "转到后台" : "关闭"}
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
              {elapsed > 0 && `已用 ${clock(elapsed)}`}
              {remaining != null && state === "running" && ` · 剩余约 ${clock(remaining)}`}
            </span>
          </div>
          <div role="progressbar" aria-valuenow={pct} aria-valuemin={0} aria-valuemax={100} aria-label="总进度" className={styles.bar}>
            <div style={{ width: `${pct}%` }} data-state={state} />
          </div>
        </div>

        <div className={styles.stages}>
          <div>
            <span>① 查词</span>
            <strong>
              {concurrency} 并发 · 进行 {counts.querying}
            </strong>
            <small>在线源限速 2 次/秒</small>
          </div>
          <div>
            <span>② 渲染</span>
            <strong>
              {counts.rendering} 个 · 排队 {live?.renderQueue ?? 0}/8
            </strong>
            <small>{(live?.renderQueue ?? 0) >= 8 ? "队列满，查词自动放慢" : "导出窗口逐个渲染"}</small>
          </div>
          <div>
            <span>③ 写入</span>
            <strong>{counts.files} 个文件</strong>
            <small>先写临时文件再改名</small>
          </div>
        </div>

        <div className={styles.memLine}>
          {live && live.memMb > 0 && (
            <span>
              内存 {live.memMb} MB / 上限 {live.memLimitMb} MB
            </span>
          )}
          {live && live.recycles > 0 && <span>渲染窗口已重建 {live.recycles} 次</span>}
          <span>进度已保存，可随时中断</span>
          <button type="button" className={styles.link} onClick={reveal}>
            打开文件夹
          </button>
        </div>

        <div className={styles.itemsBox}>
          <div role="group" aria-label="筛选" className={styles.chips}>
            {(
              [
                ["all", "全部", counts.total],
                ["active", "进行中", counts.querying + counts.rendering],
                ["noted", "有提示", counts.noted],
                ["failed", "失败", counts.failed],
              ] as const
            ).map(([f, name, n]) => (
              <button key={f} type="button" aria-pressed={filter === f} onClick={() => setFilter(f)}>
                {name}
                {f !== "all" && ` ${n}`}
              </button>
            ))}
          </div>
          <VirtualList className={styles.itemList} label="逐词状态" items={shown} rowHeight={ROW} keyOf={(i) => i.seq} render={(i) => <ItemRow item={i} />} />
        </div>
      </div>

      <div className={styles.foot}>
        {error || live?.error ? (
          <span className={styles.error} role="alert">
            {error ?? live?.error}
          </span>
        ) : (
          <span className={styles.footHint}>取消时已导出的文件会保留；完成后会生成 index.html 总目录和失败清单</span>
        )}
        {running ? (
          <>
            {confirmCancel ? (
              <>
                <button type="button" className={styles.danger} onClick={() => void cancel().then(() => setConfirmCancel(false))}>
                  确认取消
                </button>
                <button type="button" className={styles.ghostBtn} onClick={() => setConfirmCancel(false)}>
                  不取消
                </button>
              </>
            ) : (
              <button type="button" className={styles.ghostBtn} onClick={() => setConfirmCancel(true)}>
                取消
              </button>
            )}
            {state === "paused" ? (
              <button type="button" className={styles.primary} onClick={() => void resume()}>
                继续
              </button>
            ) : (
              <button type="button" className={styles.primary} onClick={() => void pause()}>
                暂停
              </button>
            )}
          </>
        ) : (
          <>
            {counts.failed > 0 && state !== "error" && (
              <button type="button" className={styles.ghostBtn} onClick={() => void retryFailed()}>
                重试失败项
              </button>
            )}
            {state === "error" && (
              <button type="button" className={styles.ghostBtn} onClick={() => void resume()}>
                继续
              </button>
            )}
            <button type="button" className={styles.ghostBtn} onClick={reveal}>
              打开文件夹
            </button>
            <button
              type="button"
              className={styles.primary}
              onClick={() => {
                backToSelect();
                onClose();
              }}
            >
              完成
            </button>
          </>
        )}
      </div>
    </>
  );
}
