// 阅读书库顶上一行：「本周阅读 3 小时 20 分 · 查看周报」；本周还没读满一分钟就不显示。

import { useEffect, useState } from "react";
import { tr } from "../i18n";
import { weeklyReport } from "../ipc/weekly";
import { formatDuration } from "./layout";
import { weekOf } from "./week";
import { useWeekly } from "./WeeklyDialog";
import styles from "./WeeklyDialog.module.css";

export function WeeklyLine() {
  const [seconds, setSeconds] = useState(0);
  useEffect(() => {
    weeklyReport(weekOf(new Date())).then(
      (r) => setSeconds(r.readSeconds),
      () => undefined,
    );
  }, []);
  if (seconds < 60) return null;
  return (
    <p className={styles.line}>
      {tr("本周阅读 {t}", { t: formatDuration(seconds) })} ·{" "}
      <button type="button" className={styles.link} onClick={() => useWeekly.getState().show(weekOf(new Date()))}>
        {tr("查看周报")}
      </button>
    </p>
  );
}
