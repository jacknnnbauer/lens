// 主窗口的后退 / 前进（鼠标侧键、Alt+← / Alt+→）：记录「确定的」查询和页面切换。
// 后退到某一步时会重新查那个词；回放引起的变化与当前这步相同，不会再记一步。

import { useCallback, useEffect, useRef } from "react";
import { useLookup } from "../../stores/lookup";
import { useRoute, type Page } from "../../stores/route";

interface Step {
  page: Page;
  /** 查词页这一步查的词 */
  query: string;
}

const MAX_STEPS = 100;

function same(a: Step, b: Step): boolean {
  return a.page === b.page && (a.page !== "lookup" || a.query === b.query);
}

export function useNavHistory(lookupWord: (w: string) => void) {
  const steps = useRef<Step[]>([]);
  const index = useRef(-1);
  const lookupRef = useRef(lookupWord);
  lookupRef.current = lookupWord;

  const record = useCallback((s: Step) => {
    const cur = steps.current[index.current];
    if (cur && same(cur, s)) {
      // 页面没变时更新这一步的词（比如在历史页时查词页的词变了）
      steps.current[index.current] = s;
      return;
    }
    // 后退后再走新路：丢掉前进的那几步
    steps.current = [...steps.current.slice(0, index.current + 1), s].slice(-MAX_STEPS);
    index.current = steps.current.length - 1;
  }, []);

  useEffect(() => {
    const current = (): Step => ({ page: useRoute.getState().page, query: useLookup.getState().committed?.query ?? "" });
    record(current());
    const offRoute = useRoute.subscribe((s, prev) => {
      // 稍后再记：从别的页面点词时先切页、再提交查询，合成一步而不是两步
      if (s.page !== prev.page) queueMicrotask(() => record(current()));
    });
    const offLookup = useLookup.subscribe((s, prev) => {
      if (s.committed && s.committed.seq !== prev.committed?.seq) record({ page: "lookup", query: s.committed.query });
    });
    return () => {
      offRoute();
      offLookup();
    };
  }, [record]);

  const move = useCallback((dir: -1 | 1) => {
    const i = index.current + dir;
    const s = steps.current[i];
    if (!s) return;
    index.current = i;
    if (s.page === "lookup" && s.query && s.query !== useLookup.getState().committed?.query) lookupRef.current(s.query);
    else useRoute.getState().go(s.page);
  }, []);

  useEffect(() => {
    // 鼠标侧键：button 3 后退、4 前进；mousedown 也拦下，避免 WebView 自己的历史导航
    const onMouse = (e: MouseEvent) => {
      if (e.button !== 3 && e.button !== 4) return;
      e.preventDefault();
      if (e.type === "mouseup") move(e.button === 3 ? -1 : 1);
    };
    const onKey = (e: KeyboardEvent) => {
      if (!e.altKey || e.ctrlKey || e.shiftKey || e.metaKey) return;
      if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
      if (document.querySelector("dialog[open]")) return;
      e.preventDefault();
      move(e.key === "ArrowLeft" ? -1 : 1);
    };
    window.addEventListener("mousedown", onMouse);
    window.addEventListener("mouseup", onMouse);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onMouse);
      window.removeEventListener("mouseup", onMouse);
      window.removeEventListener("keydown", onKey);
    };
  }, [move]);
}
