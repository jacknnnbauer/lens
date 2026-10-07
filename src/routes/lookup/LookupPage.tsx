// 查词页 · 简洁单栏（画板 05）：搜索行 → 词典跳转条 → 词典卡片。

import { useCallback, useEffect, useRef } from "react";
import { DictCard } from "../../components/DictCard";
import { SearchBox, type SearchBoxHandle } from "../../components/SearchBox";
import { useDicts } from "../../stores/dicts";
import { isExpanded, useLookup } from "../../stores/lookup";
import styles from "./LookupPage.module.css";

interface Props {
  /** 搜索行右侧的按钮（词典库、外观…） */
  actions: React.ReactNode;
  onOpenLibrary: () => void;
}

function cardToggles(): HTMLButtonElement[] {
  return Array.from(document.querySelectorAll<HTMLButtonElement>("[data-card-toggle]"));
}

export function LookupPage({ actions, onOpenLibrary }: Props) {
  const { handle, cards, done, suggestions, expanded, error, search, setExpanded } = useLookup();
  const dictCount = useDicts((s) => s.dicts.filter((d) => d.loaded).length);
  const dictsLoaded = useDicts((s) => s.loaded);
  const searchBox = useRef<SearchBoxHandle>(null);

  const lookupWord = useCallback(
    (word: string) => {
      searchBox.current?.setValue(word);
      void search(word);
      window.scrollTo({ top: 0 });
    },
    [search],
  );

  // Ctrl+K 聚焦搜索框
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey && !e.shiftKey && !e.altKey && e.key.toLowerCase() === "k") {
        e.preventDefault();
        searchBox.current?.focus();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const visible = cards.filter((c) => c.status !== "miss");
  const missed = cards.filter((c) => c.status === "miss");
  let hitRank = 0;
  const ranks = new Map(visible.map((c) => [c.id, c.status === "hit" ? hitRank++ : Number.POSITIVE_INFINITY]));
  const allMissed = done && handle && visible.length === 0;

  // Tab / Shift+Tab 在卡片之间跳转（SPEC 9.4），不进入词条内部的链接
  const onToggleKeyDown = (e: React.KeyboardEvent<HTMLButtonElement>) => {
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

  const jumpTo = (id: string) => {
    setExpanded(id, true);
    const el = document.getElementById(`dict-${id}`);
    el?.scrollIntoView({ block: "start", behavior: "smooth" });
    el?.querySelector<HTMLButtonElement>("[data-card-toggle]")?.focus({ preventScroll: true });
  };

  return (
    <div className={styles.column}>
      <div className={styles.searchRow}>
        <SearchBox ref={searchBox} onSearch={(q) => void search(q)} onTabOut={() => cardToggles()[0]?.focus()} />
        {actions}
      </div>

      {visible.length > 0 && (
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
          expanded={c.status === "pending" || isExpanded(c, ranks.get(c.id) ?? Infinity, expanded)}
          onToggle={() => setExpanded(c.id, !isExpanded(c, ranks.get(c.id) ?? Infinity, expanded))}
          onEntry={lookupWord}
          onToggleKeyDown={onToggleKeyDown}
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

      {visible.length > 0 && <p className={styles.footHint}>标题栏按 Space 展开 / 折叠 · 词条里的链接可直接查词</p>}
    </div>
  );
}
