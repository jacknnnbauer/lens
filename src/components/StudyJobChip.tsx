// 全书预习包在后台生成时，右下角的进度条：正在做什么、进度、停止；完成后「在文件夹中显示」。

import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { useStudyJob } from "../stores/studyJob";
import { CloseIcon } from "./icons";
import styles from "./StudyJobChip.module.css";
import { tr } from "../i18n";

export function StudyJobChip() {
  const job = useStudyJob((s) => s.job);
  const stop = useStudyJob((s) => s.stop);
  const dismiss = useStudyJob((s) => s.dismiss);
  if (!job) return null;
  const pct = job.progress === null ? null : Math.round(job.progress * 100);
  return (
    <div className={styles.chip} role="status" data-state={job.state}>
      {job.state === "running" && <span className={styles.spinner} aria-hidden="true" />}
      <span className={styles.text}>
        <strong>{tr("预习包 · {title}", { title: job.title })}</strong>
        <small>
          {job.state === "running"
            ? `${job.note}${pct !== null ? ` · ${pct}%` : ""}`
            : job.state === "done"
              ? tr("已生成，{n} 个词", { n: job.words.toLocaleString() })
              : job.state === "stopped"
                ? tr("已停止")
                : tr("生成失败：{msg}", { msg: job.message ?? "" })}
        </small>
        {job.state === "running" && pct !== null && (
          <span className={styles.bar} aria-hidden="true">
            <span style={{ width: `${pct}%` }} />
          </span>
        )}
      </span>
      {job.state === "running" ? (
        <button type="button" className={styles.ghost} onClick={stop}>
          {tr("停止")}
        </button>
      ) : (
        <>
          {job.state === "done" && (
            <button type="button" className={styles.ghost} onClick={() => void revealItemInDir(job.path)}>
              {tr("在文件夹中显示")}
            </button>
          )}
          <button type="button" className={styles.close} aria-label={tr("关闭")} onClick={dismiss}>
            <CloseIcon />
          </button>
        </>
      )}
    </div>
  );
}
