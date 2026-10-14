// 迷你弹窗（SPEC 8、画板 02）：常驻隐藏，取词后由 Rust 定位并显示。
// 键盘：Enter 在主窗口打开 · Tab/Shift+Tab 切换词典 · Alt+←/→ 语境句多带一句 · Ctrl+P 钉住 · Esc 关闭。
// 失焦 300ms 后自动隐藏（钉住时除外）。

import { getCurrentWindow } from "@tauri-apps/api/window";
import { useInAppLookup } from "../../components/inAppLookup";
import { useCallback, useEffect, useRef, useState } from "react";
import { ContextSentence, extend } from "../../components/ContextSentence";
import { copyCard, DEFAULT_COPY } from "../../copy/copy";
import { SearchIcon } from "../../components/icons";
import { DictBody } from "../../dict-render/DictBody";
import { hidePopup, onCapture, onCaptureUrl, openInMain, type CaptureEvent } from "../../ipc/capture";
import { Toast } from "../../components/Toast";
import { subscribeLookupEvents, useLookup } from "../../stores/lookup";
import { useSaveWord } from "../../stores/save";
import { useSettings } from "../../stores/settings";
import { useApplyTheme } from "../../theme/useApplyTheme";
import styles from "./PopupWindow.module.css";
import { tr } from "../../i18n";

const BLUR_HIDE_MS = 300;

export function PopupWindow() {
  useInAppLookup(null);
  const loadSettings = useSettings((s) => s.load);
  const { handle, cards, done, suggestions, search } = useLookup();
  useApplyTheme();

  const [capture, setCapture] = useState<CaptureEvent | null>(null);
  const [value, setValue] = useState("");
  const [tab, setTab] = useState<string | null>(null);
  const [extra, setExtra] = useState({ before: 0, after: 0 });
  const [pinned, setPinned] = useState(false);
  const pinnedRef = useRef(false);
  pinnedRef.current = pinned;
  const input = useRef<HTMLInputElement>(null);
  // 收藏时带上本次划词的语境句；改词重查后就不再是那句的语境了
  const captureMatches = capture != null && handle != null && handle.query === capture.text;
  const copyCfg = useSettings((s) => s.settings?.copy) ?? DEFAULT_COPY;
  const { saved, save } = useSaveWord(handle?.normalized || handle?.query, cards, captureMatches ? capture.contextId : null);

  const lookupWord = useCallback(
    (q: string) => {
      setValue(q);
      setTab(null);
      void search(q, { commit: true });
    },
    [search],
  );

  // 订阅：查询结果、取词、补发的 URL、窗口失焦
  useEffect(() => {
    let hideTimer: ReturnType<typeof setTimeout> | undefined;
    const subs = [
      subscribeLookupEvents(),
      onCapture((e) => {
        // 每次弹出都重读设置，主窗口里改的主题在这里也生效
        void loadSettings();
        setCapture(e);
        setExtra({ before: 0, after: 0 });
        setTab(null);
        setValue(e.text);
        // 划词是明确的查询：有命中就连同语境句记入历史
        if (e.text) void useLookup.getState().search(e.text, { commit: true, contextId: e.contextId });
        else useLookup.getState().clear();
        requestAnimationFrame(() => {
          input.current?.focus();
          input.current?.select();
        });
      }),
      onCaptureUrl(({ id, url }) => {
        setCapture((c) => (c && c.id === id ? { ...c, source: { ...c.source, url } } : c));
      }),
      getCurrentWindow().onFocusChanged(({ payload: focused }) => {
        clearTimeout(hideTimer);
        if (!focused && !pinnedRef.current) hideTimer = setTimeout(() => void hidePopup(), BLUR_HIDE_MS);
      }),
    ];
    void loadSettings();
    return () => {
      clearTimeout(hideTimer);
      for (const s of subs) void s.then((off) => off());
    };
  }, [loadSettings]);

  const hits = cards.filter((c) => c.status === "hit");
  const pending = cards.some((c) => c.status === "pending");
  const active = hits.find((c) => c.id === tab) ?? hits[0];
  const ctx = capture?.sentence ?? null;

  // Tab / Shift+Tab：滚到下一本 / 上一本词典的卡片（复制等操作针对这本）
  const cycleTab = (dir: 1 | -1) => {
    if (hits.length === 0) return;
    const i = Math.max(0, hits.findIndex((c) => c.id === active?.id));
    const next = hits[(i + dir + hits.length) % hits.length].id;
    setTab(next);
    document.getElementById(`pcard-${next}`)?.scrollIntoView({ block: "start", behavior: "smooth" });
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.nativeEvent.isComposing) return;
    if (e.key === "Escape") {
      e.preventDefault();
      void hidePopup();
    } else if (e.key === "Tab") {
      e.preventDefault();
      cycleTab(e.shiftKey ? -1 : 1);
    } else if (e.ctrlKey && e.key.toLowerCase() === "p") {
      e.preventDefault();
      setPinned((p) => !p);
    } else if (e.ctrlKey && !e.shiftKey && e.key.toLowerCase() === "s") {
      e.preventDefault();
      void save();
    } else if (e.ctrlKey && e.shiftKey && e.key.toLowerCase() === "c" && active) {
      // 复制当前词典；Markdown 末尾附上语境句（SPEC 10）
      e.preventDefault();
      const ext = ctx && captureMatches ? extend(ctx, extra.before, extra.after) : null;
      void copyCard(
        active,
        copyCfg.defaultFormat,
        copyCfg,
        ext && capture
          ? {
              sentence: ext.text,
              hlStart: ext.hlStart,
              hlEnd: ext.hlEnd,
              title: capture.source.windowTitle,
              url: capture.source.url,
              app: capture.source.app,
              date: new Date(),
            }
          : null,
      );
    } else if (e.altKey && (e.key === "ArrowLeft" || e.key === "ArrowRight") && ctx) {
      e.preventDefault();
      setExtra((x) =>
        e.key === "ArrowLeft"
          ? { ...x, before: Math.min(x.before + 1, ctx.before.length) }
          : { ...x, after: Math.min(x.after + 1, ctx.after.length) },
      );
    }
  };

  const onInputKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key !== "Enter" || e.nativeEvent.isComposing) return;
    e.preventDefault();
    const q = value.trim();
    if (!q) return;
    // 改过词就在弹窗里查；没改就展开到主窗口
    if (q !== handle?.query.trim()) lookupWord(q);
    else void openInMain(q);
  };

  const source = capture?.source;
  const sourceLine = source ? [source.app, source.windowTitle, source.url].filter(Boolean).join(" · ") : "";

  return (
    <div className={styles.popup} role="dialog" aria-label={tr("查词弹窗")} onKeyDown={onKeyDown}>
      <label className={styles.searchRow}>
        <span className={styles.icon}>
          <SearchIcon size={16} />
        </span>
        <span className="sr-only">{tr("查词")}</span>
        <input
          ref={input}
          className={styles.input}
          value={value}
          placeholder={tr("输入单词")}
          spellCheck={false}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={onInputKeyDown}
        />
        <button
          type="button"
          className={styles.pin}
          aria-pressed={saved}
          aria-label={tr(saved ? "已收藏（Ctrl+S 追加语境）" : "收藏（Ctrl+S）")}
          disabled={!hits.length}
          onClick={() => void save()}
        >
          {tr(saved ? "已收藏" : "收藏")}
        </button>
        <button
          type="button"
          className={styles.pin}
          aria-pressed={pinned}
          aria-label={tr(pinned ? "取消钉住（Ctrl+P）" : "钉住（Ctrl+P）")}
          title={tr("钉住后失焦不自动关闭")}
          onClick={() => setPinned((p) => !p)}
        >
          {tr(pinned ? "已钉住" : "钉住")}
        </button>
      </label>

      {/* 与简洁单栏一样：每本词典一张卡片，依次排列，不折叠 */}
      <div className={styles.body}>
        {hits.length > 0 ? (
          hits.map((c) => (
            <article key={c.id} id={`pcard-${c.id}`} className={styles.card} data-active={(c.id === active?.id && hits.length > 1) || undefined}>
              <h2 className={styles.cardHead}>
                {c.name}
                {c.source !== "local" && <span className={styles.cardTag}>{tr(c.source === "api" ? "在线" : "网页")}</span>}
              </h2>
              {c.entries.map((e, i) => (
                <div key={`${c.id}-${i}`} className={styles.entry}>
                  <DictBody html={e.html} onEntry={lookupWord} dictId={c.id} />
                </div>
              ))}
            </article>
          ))
        ) : !handle ? (
          <p className={styles.note}>{tr("输入单词后按 Enter 查询。")}</p>
        ) : pending || !done ? (
          <p className={styles.note}>{tr("查询中…")}</p>
        ) : (
          <div className={styles.note}>
            <p>{tr("没有找到「{q}」", { q: handle.normalized || handle.query })}</p>
            {suggestions.length > 0 && (
              <p className={styles.suggest}>
                {tr("你是不是要找：")}
                {suggestions.map((s) => (
                  <button key={s} type="button" className={styles.suggestBtn} onClick={() => lookupWord(s)}>
                    {s}
                  </button>
                ))}
              </p>
            )}
          </div>
        )}
      </div>

      {ctx && (
        <section className={styles.context} aria-label={tr("语境句")}>
          <div className={styles.contextHead}>
            <span className={styles.captured}>
              <span className={styles.dot} aria-hidden="true" />
              {tr("已捕获语境句")}
            </span>
            <button
              type="button"
              className={styles.small}
              disabled={extra.before >= ctx.before.length}
              aria-label={tr("向前多带一句（Alt+←）")}
              onClick={() => setExtra((x) => ({ ...x, before: x.before + 1 }))}
            >
              {tr("+ 前句")}
            </button>
            <button
              type="button"
              className={styles.small}
              disabled={extra.after >= ctx.after.length}
              aria-label={tr("向后多带一句（Alt+→）")}
              onClick={() => setExtra((x) => ({ ...x, after: x.after + 1 }))}
            >
              {tr("后句 +")}
            </button>
          </div>
          <ContextSentence ctx={ctx} extraBefore={extra.before} extraAfter={extra.after} />
          {sourceLine && (
            <div className={styles.source} title={sourceLine}>
              {sourceLine}
            </div>
          )}
        </section>
      )}

      <footer className={styles.footer} data-tauri-drag-region>
        <span>
          <kbd>↵</kbd> {tr("主窗口")}
        </span>
        <span>
          <kbd>Ctrl+S</kbd> {tr("收藏")}
        </span>
        <span>
          <kbd>Ctrl+Shift+C</kbd> {tr("复制")}
        </span>
        <span>
          <kbd>Ctrl+P</kbd> {tr("钉住")}
        </span>
        {ctx && (
          <span>
            <kbd>Alt+←/→</kbd> {tr("语境")}
          </span>
        )}
        <span className={styles.esc}>
          <kbd>Esc</kbd>
        </span>
      </footer>
      <Toast />
    </div>
  );
}
