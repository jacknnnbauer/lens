// 批量导出对话框（SPEC 12、画板 10/11）：没有正在看的批次时是选词页，否则是进度页。
// 另含：上次没做完的提示条、后台运行时的进度小标签。

import { useEffect, useRef } from "react";
import { useBatch } from "../../stores/batch";
import styles from "./Batch.module.css";
import { BatchProgress } from "./BatchProgress";
import { BatchSelect } from "./BatchSelect";

export function BatchDialog() {
  const open = useBatch((s) => s.open);
  const current = useBatch((s) => s.current);
  const close = useBatch((s) => s.closeDialog);
  const dialog = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const d = dialog.current;
    if (!d) return;
    if (open && !d.open) d.showModal();
    if (!open && d.open) d.close();
  }, [open]);

  return (
    <dialog ref={dialog} className={styles.dialog} onClose={close} aria-labelledby="batch-title">
      {open && (current ? <BatchProgress onClose={close} /> : <BatchSelect onClose={close} />)}
    </dialog>
  );
}

/** 启动时发现上次没做完的批次：继续 / 查看 / 放弃。 */
export function BatchResumeNotice() {
  const unfinished = useBatch((s) => s.unfinished);
  const { resume, show, discard } = useBatch.getState();
  const waiting = unfinished.filter((b) => !b.active);
  if (waiting.length === 0) return null;
  const b = waiting[0];
  return (
    <div role="status" className={styles.notice}>
      <span>
        上次的批量导出没做完：<strong>{b.label}</strong>（{b.counts.done + b.counts.failed}/{b.counts.total}）
      </span>
      <button type="button" className={styles.primarySmall} onClick={() => void resume(b.id)}>
        继续导出
      </button>
      <button type="button" className={styles.ghost} onClick={() => void show(b.id)}>
        查看
      </button>
      <button type="button" className={styles.ghost} onClick={() => void discard(b.id)}>
        放弃
      </button>
    </div>
  );
}

/** 对话框关掉、批次还在跑时，右下角显示进度，点击回到进度页。 */
export function BatchChip() {
  const open = useBatch((s) => s.open);
  const current = useBatch((s) => s.current);
  const live = useBatch((s) => s.live);
  const openDialog = useBatch((s) => s.openDialog);
  const state = live?.state ?? current?.state;
  if (open || !current || !live?.counts || (state !== "running" && state !== "paused")) return null;
  const c = live.counts;
  return (
    <button type="button" className={styles.chip} onClick={() => openDialog()}>
      <span className={styles.spinner} data-paused={state === "paused" || undefined} aria-hidden="true" />
      批量导出 {c.done + c.failed}/{c.total}
      {state === "paused" && " · 已暂停"}
    </button>
  );
}
