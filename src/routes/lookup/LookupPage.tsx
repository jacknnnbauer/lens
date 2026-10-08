// 查词页 · 简洁单栏（画板 05）：搜索行 → 词典跳转条 → 词典卡片。

import { forwardRef, useCallback, useEffect, useImperativeHandle, useRef, useState } from "react";
import { DictCard } from "../../components/DictCard";
import { ExportDialog } from "../../components/ExportDialog";
import { DownloadIcon, StarIcon } from "../../components/icons";
import { SearchBox, type SearchBoxHandle } from "../../components/SearchBox";
import { useDicts } from "../../stores/dicts";
import { useLookup } from "../../stores/lookup";
import { useSaveWord } from "../../stores/save";
import { useSettings } from "../../stores/settings";
import { toast } from "../../stores/toast";
import type { CopyFormat } from "../../copy/convert";
import { copyCard, copyMerged, DEFAULT_COPY, FORMAT_NAME } from "../../copy/copy";
import { scrollRoot } from "../../windows/main/scroll";
import styles from "./LookupPage.module.css";

const IDLE_COMMIT_MS = 1500;

export interface LookupPageHandle {
  lookupWord: (word: string) => void;
  focusSearch: () => void;
  /** 打开导出对话框（沉浸模式工具栏用）；没有命中时不打开 */
  openExport: () => void;
}

interface Props {
  /** 搜索行右侧的按钮（词典库、外观…） */
  actions: React.ReactNode;
  onOpenLibrary: () => void;
  showJumpBar: boolean;
  /** 双栏/三栏：结果上方显示大号词头（画板 01） */
  showHeadword: boolean;
}

function cardToggles(): HTMLElement[] {
  return Array.from(document.querySelectorAll<HTMLElement>("[data-card-toggle]"));
}

export const LookupPage = forwardRef<LookupPageHandle, Props>(function LookupPage(
  { actions, onOpenLibrary, showJumpBar, showHeadword },
  ref,
) {
  const { handle, cards, done, suggestions, error, search, commit, queryOnline } = useLookup();
  const dictCount = useDicts((s) => s.dicts.filter((d) => d.loaded).length);
  const dictsLoaded = useDicts((s) => s.loaded);
  const searchBox = useRef<SearchBoxHandle>(null);
  const { saved, save } = useSaveWord(handle?.normalized || handle?.query, cards);
  const saveRef = useRef(save);
  saveRef.current = save;
  const [exportOpen, setExportOpen] = useState(false);
  const canExportRef = useRef(false);
  const copyCfg = useSettings((s) => s.settings?.copy) ?? DEFAULT_COPY;
  const updateSettings = useSettings((s) => s.update);
  const setDefaultFormat = (f: CopyFormat) => {
    updateSettings({ copy: { defaultFormat: f } });
    toast(`默认复制格式：${FORMAT_NAME[f]}`);
  };

  const lookupWord = useCallback(
    (word: string) => {
      searchBox.current?.setValue(word);
      void search(word, { commit: true });
      scrollRoot().scrollTo({ top: 0 });
    },
    [search],
  );

  useImperativeHandle(
    ref,
    () => ({
      lookupWord,
      focusSearch: () => searchBox.current?.focus(),
      openExport: () => {
        if (canExportRef.current) setExportOpen(true);
      },
    }),
    [lookupWord],
  );

  // 只是打字时：结果有命中、且停留 1.5 秒没再改，才记入历史
  const anyHit = cards.some((c) => c.status === "hit");
  useEffect(() => {
    if (!handle || !done || !anyHit) return;
    const t = setTimeout(commit, IDLE_COMMIT_MS);
    return () => clearTimeout(t);
  }, [handle, done, anyHit, commit]);

  // Ctrl+K 聚焦搜索框，Ctrl+S 收藏（SPEC 9.4）
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!e.ctrlKey || e.shiftKey || e.altKey) return;
      const k = e.key.toLowerCase();
      if (k === "k") {
        e.preventDefault();
        searchBox.current?.focus();
      } else if (k === "s") {
        e.preventDefault();
        void saveRef.current();
      } else if (k === "e") {
        // Ctrl+E 导出当前词（SPEC 9.4）
        e.preventDefault();
        if (canExportRef.current) setExportOpen(true);
      }
    };
    // Ctrl+Shift+C 复制当前词典（焦点所在卡片，否则第一部展开的），Ctrl+Shift+A 合并复制所有展开的
    const onCopyKey = (e: KeyboardEvent) => {
      if (!e.ctrlKey || !e.shiftKey || e.altKey) return;
      const k = e.key.toLowerCase();
      if (k !== "c" && k !== "a") return;
      e.preventDefault();
      copyKeyRef.current(k);
    };
    window.addEventListener("keydown", onCopyKey);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("keydown", onCopyKey);
    };
  }, []);

  // 未收录的只在底部列名；本地已命中而没查的在线词典不显示
  const visible = cards.filter((c) => c.status !== "miss" && c.status !== "skipped");
  const missed = cards.filter((c) => c.status === "miss");
  const allMissed = done && handle && visible.length === 0;
  const hitCount = cards.filter((c) => c.status === "hit").length;
  const headword = cards.find((c) => c.status === "hit")?.entries[0]?.headword ?? "";
  canExportRef.current = anyHit;

  // Tab / Shift+Tab 在卡片之间跳转（SPEC 9.4），不进入词条内部的链接
  const onToggleKeyDown = (e: React.KeyboardEvent<HTMLElement>) => {
    if (e.key !== "Tab") return;
    const toggles = cardToggles();
    const i = toggles.indexOf(e.currentTarget);
    const next = e.shiftKey ? i - 1 : i + 1;
    e.preventDefault();
    if (next < 0) searchBox.current?.focus();
    else if (next < toggles.length) {
      toggles[next].focus();
      toggles[next].closest("article")?.scrollIntoView({ block: "nearest", behavior: "smooth" });
    }
  };

  const expandedHits = visible.filter((c) => c.status === "hit");
  const copyKeyRef = useRef<(k: string) => void>(() => undefined);
  copyKeyRef.current = (k) => {
    if (k === "a") {
      void copyMerged(expandedHits, copyCfg.defaultFormat, copyCfg);
      return;
    }
    const focusedId = (document.activeElement?.closest("article")?.id ?? "").replace(/^dict-/, "");
    const card = cards.find((c) => c.id === focusedId && c.status === "hit") ?? expandedHits[0];
    if (card) void copyCard(card, copyCfg.defaultFormat, copyCfg);
  };

  const jumpTo = (id: string) => {
    const el = document.getElementById(`dict-${id}`);
    el?.scrollIntoView({ block: "start", behavior: "smooth" });
    el?.querySelector<HTMLElement>("[data-card-toggle]")?.focus({ preventScroll: true });
  };

  return (
    <div className={styles.column}>
      <div className={styles.searchRow}>
        <SearchBox ref={searchBox} onSearch={(q, c) => void search(q, { commit: c })} onTabOut={() => cardToggles()[0]?.focus()} />
        <button
          type="button"
          className={styles.star}
          aria-pressed={saved}
          aria-label={saved ? "已收藏（Ctrl+S 追加语境）" : "收藏到生词本（Ctrl+S）"}
          title={saved ? "已收藏 · Ctrl+S 追加语境" : "收藏到生词本 · Ctrl+S"}
          disabled={!anyHit}
          onClick={() => void save()}
        >
          <StarIcon filled={saved} />
        </button>
        <button
          type="button"
          className={styles.star}
          aria-label="导出 PDF 或 HTML（Ctrl+E）"
          title="导出 PDF 或 HTML · Ctrl+E"
          disabled={!anyHit}
          onClick={() => setExportOpen(true)}
        >
          <DownloadIcon />
        </button>
        {actions}
      </div>

      {showHeadword && headword && (
        <header className={styles.headword}>
          <h1>{headword}</h1>
          <span>
            {hitCount}/{cards.length} 部词典命中
          </span>
        </header>
      )}

      {showJumpBar && visible.length > 0 && (
        <nav aria-label="跳转到词典" className={styles.jump}>
          {visible.map((c) => (
            <a
              key={c.id}
              href={`#dict-${c.id}`}
              className={styles.pill}
              data-pending={c.status === "pending" || undefined}
              onClick={(e) => {
                e.preventDefault();
                jumpTo(c.id);
              }}
            >
              {c.name}
            </a>
          ))}
          <span className={styles.jumpHint}>Tab / Shift+Tab 跳转</span>
        </nav>
      )}

      {error && (
        <p role="alert" className={styles.error}>
          {error}
        </p>
      )}

      {visible.map((c) => (
        <DictCard
          key={c.id}
          card={c}
          onEntry={lookupWord}
          onToggleKeyDown={onToggleKeyDown}
          copyFormat={copyCfg.defaultFormat}
          onCopy={(f) => void copyCard(c, f, copyCfg)}
          onSetDefaultFormat={setDefaultFormat}
          onQueryOnline={() => void queryOnline(c.id)}
        />
      ))}

      {allMissed && (
        <section className={styles.empty} aria-live="polite">
          <p className={styles.emptyTitle}>没有找到「{handle.normalized || handle.query}」</p>
          {suggestions.length > 0 && (
            <div className={styles.suggest}>
              <span>你是不是要找：</span>
              {suggestions.map((s) => (
                <button key={s} type="button" className={styles.suggestBtn} onClick={() => lookupWord(s)}>
                  {s}
                </button>
              ))}
            </div>
          )}
        </section>
      )}

      {visible.length > 0 && missed.length > 0 && done && (
        <p className={styles.missed}>未收录：{missed.map((c) => c.name).join("、")}</p>
      )}

      {!handle && dictsLoaded && (
        <section className={styles.empty}>
          {dictCount === 0 ? (
            <>
              <p className={styles.emptyTitle}>还没有词典</p>
              <p className={styles.emptySub}>支持 MDict（.mdx/.mdd）、StarDict（.ifo）和 Lens JSON 词典。</p>
              <button type="button" className={styles.primary} onClick={onOpenLibrary}>
                导入词典…
              </button>
            </>
          ) : (
            <p className={styles.emptySub}>
              {dictCount} 部词典就绪 · 输入单词开始查询 · <kbd>Ctrl+K</kbd> 聚焦搜索框
            </p>
          )}
        </section>
      )}

      <ExportDialog open={exportOpen} onClose={() => setExportOpen(false)} word={headword || handle?.normalized || ""} cards={cards} />

      {visible.length > 0 && <p className={styles.footHint}>Tab / Shift+Tab 在词典之间跳转 · Ctrl+Shift+C 复制当前词典 · Ctrl+Shift+A 合并复制全部词典</p>}
    </div>
  );
});
