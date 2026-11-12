// 查词页 · 简洁单栏（画板 05）：搜索行 → 词典跳转条 → 词典卡片。

import { forwardRef, useCallback, useEffect, useImperativeHandle, useRef, useState } from "react";
import { WordFamily } from "../../components/WordFamily";
import { ReverseStrip } from "../../components/ReverseStrip";
import { ContextSenseCard, type SentenceCtx } from "../../components/ContextSenseCard";
import { getContext } from "../../ipc/history";
import { CardWall } from "../../components/CardWall";
import { DictCard } from "../../components/DictCard";
import { ExportDialog } from "../../components/ExportDialog";
import { DownloadIcon, StarIcon } from "../../components/icons";
import { SearchBox, type SearchBoxHandle } from "../../components/SearchBox";
import { isHanPhrase, isSentence } from "../../components/sentence";
import { Hint } from "../../components/Hint";
import { normalizeHeadword } from "./alsoForm";
import { SenseAtlas } from "../../components/SenseAtlas";
import { useDicts } from "../../stores/dicts";
import { useImmersive } from "../../stores/immersive";
import { useLookup } from "../../stores/lookup";
import { useRoute } from "../../stores/route";
import { useSaveWord } from "../../stores/save";
import { pressed, useKeyLabel } from "../../keys";
import { useSettings } from "../../stores/settings";
import { toast } from "../../stores/toast";
import type { CopyFormat } from "../../copy/convert";
import { copyCard, copyMerged, DEFAULT_COPY, FORMAT_NAME } from "../../copy/copy";
import { scrollRoot } from "../../windows/main/scroll";
import styles from "./LookupPage.module.css";
import { tr } from "../../i18n";

export interface LookupPageHandle {
  /** contextId：窗口内划词时存下的语境句，收藏时带上 */
  lookupWord: (word: string, contextId?: number | null) => void;
  focusSearch: () => void;
  /** 打开导出对话框（沉浸模式工具栏用）；没有命中时不打开 */
  openExport: () => void;
}

interface Props {
  /** 搜索行右侧的按钮（词典库、外观…） */
  actions: React.ReactNode;
  onOpenLibrary: () => void;
  /** 反查条的「全部」：到反查与全文页 */
  onOpenSearch: () => void;
  showJumpBar: boolean;
  /** 双栏/三栏：结果上方显示大号词头（画板 01） */
  showHeadword: boolean;
}

function cardToggles(): HTMLElement[] {
  return Array.from(document.querySelectorAll<HTMLElement>("[data-card-toggle]"));
}

export const LookupPage = forwardRef<LookupPageHandle, Props>(function LookupPage(
  { actions, onOpenLibrary, onOpenSearch, showJumpBar, showHeadword },
  ref,
) {
  const { handle, cards, done, suggestions, error, search, queryOnline } = useLookup();
  const dictCount = useDicts((s) => s.dicts.filter((d) => d.loaded).length);
  const dictsLoaded = useDicts((s) => s.loaded);
  const searchBox = useRef<SearchBoxHandle>(null);
  const { saved, save, enabled: canSave } = useSaveWord(handle?.normalized || handle?.query, cards);
  const saveKey = useKeyLabel("save");
  const exportKey = useKeyLabel("export");
  const saveRef = useRef(save);
  saveRef.current = save;
  // 设置 › 取词「主窗口查词时加入生词本：自动」：确定的查询有命中就收藏一次
  const autoSaveLookup = useSettings((s) => s.settings?.wordbook?.autoSaveLookup) ?? false;
  const committed = useLookup((s) => s.committed);
  const autoSavedSeq = useRef<number | null>(null);
  const hasHit = cards.some((c) => c.status === "hit");
  useEffect(() => {
    if (!autoSaveLookup || !committed || !handle || committed.query !== handle.query.trim() || !hasHit) return;
    if (autoSavedSeq.current === committed.seq) return;
    autoSavedSeq.current = committed.seq;
    void saveRef.current();
  }, [autoSaveLookup, committed, handle, hasHit]);
  const [exportOpen, setExportOpen] = useState(false);
  const canExportRef = useRef(false);
  const copyCfg = useSettings((s) => s.settings?.copy) ?? DEFAULT_COPY;
  const showAtlas = useSettings((s) => s.settings?.layout.showSenseAtlas) ?? false;
  // 卡片墙里拖动换位：当前结果立即换位，同时改词典库里的先后（下次查词也按这个顺序）
  const reorderCard = useCallback((key: string, target: string, after: boolean) => {
    const move = <T,>(list: T[], id: (x: T) => string): T[] => {
      const from = list.find((x) => id(x) === key);
      if (!from) return list;
      const rest = list.filter((x) => id(x) !== key);
      const at = rest.findIndex((x) => id(x) === target);
      if (at < 0) return list;
      rest.splice(after ? at + 1 : at, 0, from);
      return rest;
    };
    useLookup.setState((s) => ({ cards: move(s.cards, (c) => c.id) }));
    void useDicts.getState().setOrderQuiet(move(useDicts.getState().dicts, (d) => d.id).map((d) => d.id));
  }, []);
  const wall = useSettings((s) => s.settings?.layout.cardWall) ?? false;
  const wallColumn = useSettings((s) => s.settings?.layout.wallColumn) ?? 380;
  const wallMaxHeight = useSettings((s) => s.settings?.layout.wallMaxHeight) ?? 0;
  const updateSettings = useSettings((s) => s.update);
  const setDefaultFormat = (f: CopyFormat) => {
    updateSettings({ copy: { defaultFormat: f } });
    toast(tr("默认复制格式：{format}", { format: tr(FORMAT_NAME[f]) }));
  };

  const lookupWord = useCallback(
    (word: string, contextId?: number | null) => {
      searchBox.current?.setValue(word);
      void search(word, { commit: true, contextId: contextId ?? null });
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

  // 历史只记明确发起的查询（Enter、选候选、点词条链接、划词），打字过程中的 n / nu / nua 不记
  const anyHit = cards.some((c) => c.status === "hit");

  // 「当前语境」的句子：划词 / 窗口内查词捕获的语境句，或者自己粘贴的一句话（只对当时选的那个词有效）
  const showContextSense = useSettings((s) => s.settings?.layout.showContextSense) ?? true;
  const showWordFamily = useSettings((s) => s.settings?.layout.showWordFamily) ?? true;
  const contextId = useLookup((s) => s.contextId);
  const [captured, setCaptured] = useState<SentenceCtx | null>(null);
  const [pasted, setPasted] = useState<(SentenceCtx & { query: string }) | null>(null);
  useEffect(() => {
    setCaptured(null);
    if (contextId == null) return;
    let alive = true;
    getContext(contextId).then(
      (c) => alive && setCaptured(c && { sentence: c.sentence, hlStart: c.hlStart, hlEnd: c.hlEnd }),
      () => undefined,
    );
    return () => {
      alive = false;
    };
  }, [contextId]);
  const sentenceCtx = pasted && handle && pasted.query === handle.query ? pasted : captured;

  // 搜索框里粘贴了一整句：不查整句，等在句中点词（SearchBox 选中那个词），查它并用这句话判断词义
  const onSearchBox = (q: string, commit: boolean) => {
    void search(isSentence(q) ? "" : q, { commit });
  };
  const pickInSentence = (sentence: string, hlStart: number, hlEnd: number) => {
    const q = sentence.slice(hlStart, hlEnd);
    setPasted({ sentence, hlStart, hlEnd, query: q });
    void search(q, { commit: true });
    scrollRoot().scrollTo({ top: 0 });
  };

  // 在查词页上不必先按 Ctrl+K：焦点不在输入框时，直接打字会落进搜索框（替换掉上一个词），Enter 回到搜索框并全选
  useEffect(() => {
    const onType = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.ctrlKey || e.altKey || e.metaKey || e.isComposing) return;
      if (useRoute.getState().page !== "lookup" || useImmersive.getState().on) return;
      if (document.querySelector("dialog[open]")) return;
      const t = e.target as HTMLElement | null;
      if (t && (t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName))) return;
      const printable = e.key.length === 1 && e.key !== " ";
      if (e.key === "Enter") {
        // 按钮、链接上的 Enter 照常生效
        if (t && /^(BUTTON|A|SUMMARY)$/.test(t.tagName)) return;
        e.preventDefault();
        searchBox.current?.focus();
      } else if (printable) {
        // 这个键由搜索框接手（替换掉原来的词）；之后焦点已在输入框里，后续按键照常输入
        e.preventDefault();
        searchBox.current?.startTyping(e.key);
      }
    };
    window.addEventListener("keydown", onType);
    return () => window.removeEventListener("keydown", onType);
  }, []);

  // 聚焦搜索框、收藏、导出、复制当前 / 全部词典（SPEC 9.4；组合键可在设置里改，见 src/keys.ts）
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (pressed(e, "focusSearch")) {
        e.preventDefault();
        searchBox.current?.focus();
      } else if (pressed(e, "save")) {
        e.preventDefault();
        void saveRef.current();
      } else if (pressed(e, "export")) {
        e.preventDefault();
        if (canExportRef.current) setExportOpen(true);
      } else if (pressed(e, "copyCurrent")) {
        // 焦点所在卡片，否则第一部展开的
        e.preventDefault();
        copyKeyRef.current("c");
      } else if (pressed(e, "copyAll")) {
        e.preventDefault();
        copyKeyRef.current("a");
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // 未收录的只在底部列名；本地已命中而没查的在线词典不显示
  const visible = cards.filter((c) => c.status !== "miss" && c.status !== "skipped");
  const missed = cards.filter((c) => c.status === "miss");
  const allMissed = done && handle && visible.length === 0;
  const hitCount = cards.filter((c) => c.status === "hit").length;
  const headword = cards.find((c) => c.status === "hit")?.entries[0]?.headword ?? "";
  // 查 left 时词典给的是 left：提示它也可能是 leave 的变形（结果里已经有 leave 的就不提示）
  const base = handle?.base ?? null;
  const otherForm =
    base && done && hitCount > 0 && !cards.some((c) => c.entries.some((e) => normalizeHeadword(e.headword) === base)) ? base : null;
  const missingDicts = useDicts((s) => s.dicts.filter((d) => d.missing).length);
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
    <div
      className={styles.column}
      data-wall={wall || undefined}
      style={wall && wallMaxHeight > 0 ? ({ "--card-max-h": `${wallMaxHeight}px`, "--card-overflow": "auto" } as React.CSSProperties) : undefined}
    >
      <div className={styles.searchRow}>
        <SearchBox ref={searchBox} onSearch={onSearchBox} onPickInSentence={pickInSentence} onTabOut={() => cardToggles()[0]?.focus()} />
        <button
          type="button"
          className={styles.star}
          data-ui="tool"
          aria-pressed={saved}
          aria-label={tr(saved ? "已收藏（{key} 追加语境）" : "收藏到生词本（{key}）", { key: saveKey })}
          title={tr(saved ? "已收藏 · {key} 追加语境" : "收藏到生词本 · {key}", { key: saveKey })}
          disabled={!canSave}
          onClick={() => void save()}
        >
          <StarIcon filled={saved} />
        </button>
        <button
          type="button"
          className={styles.star}
          data-ui="tool"
          aria-label={tr("导出 PDF 或 HTML（{key}）", { key: exportKey })}
          title={tr("导出 PDF 或 HTML · {key}", { key: exportKey })}
          disabled={!anyHit}
          onClick={() => setExportOpen(true)}
        >
          <DownloadIcon />
        </button>
        {actions}
      </div>

      {showHeadword && headword && (
        <header className={styles.headword} data-ui="headword">
          <h1>{headword}</h1>
          <span>
            {tr("{hits}/{total} 部词典命中", { hits: hitCount, total: cards.length })}
          </span>
        </header>
      )}

      {otherForm && handle && (
        <p className={styles.otherForm}>
          {tr("{w} 也可能是 {base} 的变形", { w: handle.normalized || handle.query.trim() })
            .split("{base}")
            .flatMap((part, i) =>
              i === 0
                ? [part]
                : [
                    <button key="b" type="button" className={styles.otherFormLink} onClick={() => lookupWord(otherForm)}>
                      {otherForm}
                    </button>,
                    part,
                  ],
            )}
        </p>
      )}

      {missingDicts > 0 && (
        <p className={styles.missingNote}>
          {tr("有 {n} 部词典找不到文件，查词时不出现。", { n: missingDicts })}
          <button type="button" className={styles.otherFormLink} onClick={onOpenLibrary}>
            {tr("去词典库重新指定")}
          </button>
        </p>
      )}

      <Hint id="han-char" anchor="search" show={!!handle && done && hitCount > 0 && isHanPhrase(handle.query) && [...handle.query.trim()].length >= 2}>
        {tr("点框里的某一个字，可以只查这个字。")}
      </Hint>

      {showJumpBar && visible.length > 0 && (
        <nav aria-label={tr("跳转到词典")} className={styles.jump}>
          {visible.map((c) => (
            <a
              key={c.id}
              href={`#dict-${c.id}`}
              className={styles.pill}
              data-ui="chip"
              data-pending={c.status === "pending" || undefined}
              onClick={(e) => {
                e.preventDefault();
                jumpTo(c.id);
              }}
            >
              {c.name}
            </a>
          ))}
        </nav>
      )}

      {error && (
        <p role="alert" className={styles.error}>
          {error}
        </p>
      )}

      {handle && <ReverseStrip query={handle.query} onLookup={lookupWord} onOpenSearch={onOpenSearch} />}

      {/* 在粘贴的句子里点词时，没查到也留着句子，好换别的词（中文词多半只有反查） */}
      {showContextSense && handle && (hitCount > 0 || (pasted && sentenceCtx === pasted)) && (
        <ContextSenseCard
          word={headword || handle.normalized || handle.query}
          cards={cards}
          ctx={sentenceCtx}
          onPick={(w, c) => {
            setPasted({ ...c, query: w });
            lookupWord(w);
          }}
          allowPaste={false}
          onPickSpan={
            pasted && sentenceCtx === pasted
              ? (s, e) => {
                  searchBox.current?.selectInSentence(s, e);
                  pickInSentence(pasted.sentence, s, e);
                }
              : undefined
          }
        />
      )}

      {showWordFamily && handle && done && hitCount > 0 && <WordFamily word={headword || handle.normalized || handle.query} onLookup={lookupWord} />}

      {showAtlas && done && hitCount >= 2 && <SenseAtlas word={handle?.normalized || handle?.query || headword} cards={cards} onJump={jumpTo} />}

      {(() => {
        const cardOf = (c: (typeof visible)[number]) => (
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
        );
        // 卡片墙：按卡片自身高度铺满多列；否则上下一列
        return wall ? (
          <CardWall minColumn={wallColumn} items={visible.map((c) => ({ key: c.id, node: cardOf(c) }))} onReorder={reorderCard} />
        ) : (
          visible.map(cardOf)
        );
      })()}

      {allMissed && (
        <section className={styles.empty} aria-live="polite">
          <p className={styles.emptyTitle}>{tr("没有找到「{q}」", { q: handle.normalized || handle.query })}</p>
          {suggestions.length > 0 && (
            <div className={styles.suggest}>
              <span>{tr("你是不是要找：")}</span>
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
        <p className={styles.missed}>{tr("未收录：{names}", { names: missed.map((c) => c.name).join(tr("、")) })}</p>
      )}

      {!handle && dictsLoaded && (
        <section className={styles.empty}>
          {dictCount === 0 ? (
            <>
              <p className={styles.emptyTitle}>{tr("还没有词典")}</p>
              <p className={styles.emptySub}>{tr("支持 MDict（.mdx/.mdd）、StarDict（.ifo）和 Lens JSON 词典。")}</p>
              <button type="button" className={styles.primary} onClick={onOpenLibrary}>
                {tr("导入词典…")}
              </button>
            </>
          ) : (
            <p className={styles.emptySub}>
              {tr("{n} 部词典就绪 · 输入单词开始查询", { n: dictCount })} · <kbd>Ctrl+K</kbd> {tr("聚焦搜索框")}
            </p>
          )}
        </section>
      )}

      <ExportDialog open={exportOpen} onClose={() => setExportOpen(false)} word={headword || handle?.normalized || ""} cards={cards} />

    </div>
  );
});
