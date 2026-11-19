// 正在整理 PDF 的进度条（主窗口底部浮着，可取消）

import { tr } from "../../i18n";
import { usePdfImport } from "../../stores/pdfImport";
import styles from "./PdfImportProgress.module.css";

export function PdfImportProgress() {
  const running = usePdfImport((s) => s.running);
  const queued = usePdfImport((s) => s.queue.length);
  const cancel = usePdfImport((s) => s.cancel);
  if (!running) return null;
  const pct = running.total ? Math.round((running.done / running.total) * 100) : 0;
  return (
    <div className={styles.box} role="status" aria-live="polite" data-pdf-import="">
      <div className={styles.text}>
        <b title={running.name}>{running.name}</b>
        <span>
          {running.total ? tr("正在整理 PDF 第 {n} / {total} 页", { n: running.done, total: running.total }) : tr("正在打开 PDF…")}
          {queued > 0 && ` · ${tr("还有 {n} 个排队", { n: queued })}`}
        </span>
      </div>
      <div className={styles.bar}>
        <span style={{ width: `${pct}%` }} />
      </div>
      <button type="button" className={styles.cancel} onClick={cancel}>
        {tr("取消")}
      </button>
    </div>
  );
}
