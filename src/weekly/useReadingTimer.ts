// 阅读页的计时：开着一篇文章 / 一本书时每秒 tick；在阅读页、页面可见、窗口有焦点才算，60 秒没操作就停。
// 攒满 30 秒、换文章、离开阅读页时写进 reading_log（今天的本地日期）。

import { useEffect, useRef } from "react";
import { readingLogAdd } from "../ipc/weekly";
import { useRoute } from "../stores/route";
import { ReadingTimer } from "./timer";
import { dayOf } from "./week";

export function useReadingTimer(articleId: number | undefined, enabled: boolean) {
  const timer = useRef<ReadingTimer | null>(null);
  useEffect(() => {
    if (articleId === undefined || !enabled) return;
    const t = new ReadingTimer(
      () => Date.now(),
      (s, w) => void readingLogAdd(dayOf(new Date()), articleId, s, w).catch(() => undefined),
    );
    timer.current = t;
    const act = () => t.activity();
    const sync = () => t.setActive(useRoute.getState().page === "reader" && document.visibilityState === "visible" && document.hasFocus());
    t.activity();
    sync();
    const iv = setInterval(() => {
      sync();
      t.tick();
    }, 1000);
    const opts = { passive: true, capture: true } as const;
    const events = ["mousemove", "wheel", "keydown", "mousedown", "scroll"] as const;
    for (const e of events) window.addEventListener(e, act, opts);
    window.addEventListener("focus", sync);
    window.addEventListener("blur", sync);
    document.addEventListener("visibilitychange", sync);
    return () => {
      clearInterval(iv);
      for (const e of events) window.removeEventListener(e, act, opts);
      window.removeEventListener("focus", sync);
      window.removeEventListener("blur", sync);
      document.removeEventListener("visibilitychange", sync);
      t.stop();
      timer.current = null;
    };
  }, [articleId, enabled]);
  return timer;
}
