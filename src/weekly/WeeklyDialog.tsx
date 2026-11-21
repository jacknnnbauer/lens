// 阅读周报窗口：画好的卡片预览，左右翻周、换一句金句、存成图片 / 复制图片。
// 谁都能打开它：useWeekly.getState().show(week)（书库顶上那一行、每周第一次的提示）。

import { useEffect, useRef, useState } from "react";
import { create } from "zustand";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { tr } from "../i18n";
import { weeklyReport, weeklySavePng, type WeeklyReport } from "../ipc/weekly";
import { toast } from "../stores/toast";
import { canvasMeasure, drawCard } from "./drawCard";
import { H, layoutCard, W } from "./layout";
import { shiftWeek, weekOf, type Week } from "./week";
import styles from "./WeeklyDialog.module.css";

export const useWeekly = create<{ week: Week | null; show: (w: Week) => void; close: () => void }>((set) => ({
  week: null,
  show: (week) => set({ week }),
  close: () => set({ week: null }),
}));

/** 这份周报有没有能画的东西 */
export function hasContent(r: WeeklyReport): boolean {
  return r.readSeconds >= 60 || !!r.vocab || r.newWords > 0 || r.quotes.length > 0;
}

export function WeeklyDialog() {
  const week = useWeekly((s) => s.week);
  const { show, close } = useWeekly.getState();
  const [report, setReport] = useState<WeeklyReport | null>(null);
  const [quote, setQuote] = useState(0);
  const [drawn, setDrawn] = useState(false);
  const canvas = useRef<HTMLCanvasElement>(null);
  const thisWeek = weekOf(new Date());

  useEffect(() => {
    if (!week) return;
    let alive = true;
    setReport(null);
    setQuote(0);
    weeklyReport(week).then(
      (r) => alive && setReport(r),
      (e) => alive && toast(String(e)),
    );
    return () => {
      alive = false;
    };
  }, [week]);

  // 画卡片（等字体加载好，免得第一次画出来是替代字体）
  useEffect(() => {
    const el = canvas.current;
    if (!week || !report || !el) return;
    let alive = true;
    setDrawn(false);
    void document.fonts.ready.then(() => {
      const ctx = el.getContext("2d");
      if (!alive || !ctx) return;
      const layout = layoutCard(report, quote, canvasMeasure(ctx), week.fromDay);
      if (!layout) return;
      // 改尺寸会清空画布、重置状态，所以先改再画
      el.height = layout.height;
      drawCard(ctx, layout, week);
      setDrawn(true);
    });
    return () => {
      alive = false;
    };
  }, [week, report, quote]);

  useEffect(() => {
    if (!week) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
      else if (e.key === "ArrowLeft") show(shiftWeek(week, -1));
      else if (e.key === "ArrowRight" && week.fromMs < thisWeek.fromMs) show(shiftWeek(week, 1));
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [week, close, show, thisWeek.fromMs]);

  if (!week) return null;
  const empty = report !== null && !hasContent(report);
  const end = new Date(week.end.getTime() - 86_400_000);
  const md = (d: Date) => `${d.getMonth() + 1}.${d.getDate()}`;

  const saveImage = async () => {
    const el = canvas.current;
    if (!el) return;
    const path = await saveDialog({ defaultPath: `Lens 周报 ${week.year}-W${String(week.week).padStart(2, "0")}.png`, filters: [{ name: "PNG", extensions: ["png"] }] });
    if (!path) return;
    try {
      await weeklySavePng(path, el.toDataURL("image/png"));
      toast(tr("已保存周报图片"));
    } catch (e) {
      toast(String(e));
    }
  };

  const copyImage = async () => {
    const el = canvas.current;
    if (!el) return;
    try {
      const blob = await new Promise<Blob>((res, rej) => el.toBlob((b) => (b ? res(b) : rej(new Error("toBlob"))), "image/png"));
      await navigator.clipboard.write([new ClipboardItem({ "image/png": blob })]);
      toast(tr("已复制，可以直接粘贴到微信"));
    } catch (e) {
      toast(String(e));
    }
  };

  return (
    <div className={styles.backdrop} onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div className={styles.dialog} role="dialog" aria-modal="true" aria-label={tr("阅读周报")}>
        <header className={styles.head}>
          <button type="button" className={styles.nav} aria-label={tr("上一周")} title={tr("上一周（←）")} onClick={() => show(shiftWeek(week, -1))}>
            ‹
          </button>
          <span className={styles.range}>
            {tr("第 {week} 周 · {from} – {to}", { week: week.week, from: md(week.start), to: md(end) })}
          </span>
          <button
            type="button"
            className={styles.nav}
            aria-label={tr("下一周")}
            title={tr("下一周（→）")}
            disabled={week.fromMs >= thisWeek.fromMs}
            onClick={() => show(shiftWeek(week, 1))}
          >
            ›
          </button>
          <span className={styles.spacer} />
          <button type="button" className={styles.close} aria-label={tr("关闭")} onClick={close}>
            ×
          </button>
        </header>

        <div className={styles.stage}>
          {empty ? (
            <p className={styles.empty}>{tr("这周还没有阅读记录")}</p>
          ) : (
            <canvas ref={canvas} width={W} height={H} className={styles.card} data-ready={drawn || undefined} />
          )}
        </div>

        {!empty && (
          <footer className={styles.foot}>
            {report && report.quotes.length > 1 && (
              <button type="button" className={styles.ghost} onClick={() => setQuote((q) => q + 1)}>
                {tr("换一句")}
              </button>
            )}
            <span className={styles.spacer} />
            <button type="button" className={styles.ghost} disabled={!drawn} onClick={() => void copyImage()}>
              {tr("复制图片")}
            </button>
            <button type="button" className={styles.primary} disabled={!drawn} onClick={() => void saveImage()}>
              {tr("存成图片")}
            </button>
          </footer>
        )}
      </div>
    </div>
  );
}
