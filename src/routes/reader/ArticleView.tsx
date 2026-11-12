// 阅读视图：中间是排好版的文章；「释义 / 生词」放在主窗口的右侧栏里（标题栏右上角开关）。
// 点一个词（或拖选一个短语）就在右边查；所在句子存为语境句，查到后自动收进生词本（可关）。
// 生词按词频标在正文上（可关），「生词」页列出全文的生词，可批量收藏、标记认识。
// 双击正文空白处进入编辑（所见即所得 / 源码，只改 Lens 里的这份），保存后重新排版、重新扫描。
// 「导出」把标注过的原文和词汇表导出成 PDF / HTML（ArticleExportDialog）。
// 电子书一次显示一章：生词、导出都按当前这一章；点工具栏上的章节名打开目录，正文下面翻上一章 / 下一章。书不能编辑。

import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { DictCard } from "../../components/DictCard";
import { Hint, markHint } from "../../components/Hint";
import { acceptable, paragraphOf } from "../../components/inAppLookup";
import { bookChapter, openArticle, openBook, renameArticle, saveReadingContext, setArticleProgress, setBookProgress, updateArticleBody, type Article, type BookInfo, type TocItem } from "../../ipc/articles";
import { wordGlosses, type WordGloss } from "../../ipc/lookup";
import { clearKnownWords, importKnownWords, saveScannedWords, scanText, setKnownWords, type ScanResult, type ScanStatus, type ScanWord } from "../../ipc/scan";
import { saveWord, wordId } from "../../ipc/words";
import { copyCard, DEFAULT_COPY, FORMAT_NAME } from "../../copy/copy";
import type { CopyFormat } from "../../copy/convert";
import { useLookup } from "../../stores/lookup";
import { useReader } from "../../stores/reader";
import { useRoute } from "../../stores/route";
import { briefOf } from "../../stores/save";
import { useSettings } from "../../stores/settings";
import { toast } from "../../stores/toast";
import { scrollRoot } from "../../windows/main/scroll";
import { ArticleExportDialog } from "./ArticleExportDialog";
import { BookStudyDialog } from "./BookStudyDialog";
import { Bookmarks } from "./Bookmarks";
import { WysiwygEditor, type WysiwygApi } from "./WysiwygEditor";
import { renderBookChapter, renderMarkdown, resolveBookPath, WORD_CLASS, wrapWords, type WordIndex } from "./markdown";
import styles from "./ReaderPage.module.css";
import { tr } from "../../i18n";

const LEVELS = [2000, 3000, 5000, 8000, 12000, 20000];
const GLOSS_BATCH = 80;

const imageLabel = (alt: string) => (alt ? tr("［图片：{alt}］", { alt }) : tr("［图片］"));

function anchorTarget(el: HTMLElement, anchor: string): Element | null {
  const quoted = CSS.escape(anchor);
  return el.querySelector(`[id="${quoted}"], [name="${quoted}"]`);
}

/** 滚到正文里的锚点；没有这个锚点时返回 false */
function scrollToAnchor(el: HTMLElement | null, anchor: string | null | undefined): boolean {
  const target = el && anchor ? anchorTarget(el, anchor) : null;
  target?.scrollIntoView({ block: "start" });
  return !!target;
}

/** 目录里正在读的那一项：这一章之前的最后一项，或这一章里已经滚过的最后一个锚点 */
function tocIndexAt(toc: TocItem[], chapter: number, el: HTMLElement, viewTop: number): number {
  let found = -1;
  toc.forEach((t, i) => {
    if (t.chapter > chapter) return;
    if (t.chapter === chapter && t.anchor) {
      const target = anchorTarget(el, t.anchor);
      if (target && target.getBoundingClientRect().top > viewTop + 80) return;
    }
    found = i;
  });
  return found;
}

function readPref<T>(key: string, fallback: T, valid: (v: unknown) => boolean): T {
  try {
    const raw = localStorage.getItem(key);
    const v = raw == null ? null : JSON.parse(raw);
    return valid(v) ? (v as T) : fallback;
  } catch {
    return fallback;
  }
}

function writePref(key: string, v: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(v));
  } catch {
    // 记不住只是下次回到默认值
  }
}

interface Props {
  id: number;
  onBack: () => void;
}

interface Picked {
  word: string;
  contextId: number | null;
  /** 点到的词在扫描里的原形（depleted → deplete）；拖选的短语、手动改过的词没有 */
  lemma?: string | null;
  at: number;
}

/** 这个点是不是落在某个字上（双击空白处才进入编辑，双击文字不算） */
function onText(x: number, y: number): boolean {
  const r = document.caretRangeFromPoint?.(x, y);
  const node = r?.startContainer;
  if (!r || !node || node.nodeType !== Node.TEXT_NODE) return false;
  const text = node.textContent ?? "";
  for (const off of [r.startOffset - 1, r.startOffset]) {
    if (off < 0 || off >= text.length || !text[off].trim()) continue;
    const c = document.createRange();
    c.setStart(node, off);
    c.setEnd(node, off + 1);
    for (const b of Array.from(c.getClientRects())) {
      if (x >= b.left - 2 && x <= b.right + 2 && y >= b.top - 2 && y <= b.bottom + 2) return true;
    }
  }
  return false;
}

export function ArticleView({ id, onBack }: Props) {
  const [article, setArticle] = useState<Article | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [level, setLevel] = useState<number>(() => readPref("lens.scan.level", 5000, (v) => LEVELS.includes(v as number)));
  const [marks, setMarks] = useState<boolean>(() => readPref("lens.reader.marks", true, (v) => typeof v === "boolean"));
  const [autoSave, setAutoSave] = useState<boolean>(() => readPref("lens.reader.autoSave", true, (v) => typeof v === "boolean"));
  const [tab, setTab] = useState<"dict" | "words">("words");
  const [scan, setScan] = useState<ScanResult | null>(null);
  const [picked, setPicked] = useState<Picked | null>(null);
  const [editingTitle, setEditingTitle] = useState(false);
  // 正在编辑的原文；null 表示在阅读
  const [draft, setDraft] = useState<string | null>(null);
  const [confirmDiscard, setConfirmDiscard] = useState(false);
  const [savingBody, setSavingBody] = useState(false);
  // 编辑方式：所见即所得 / Markdown 源码
  const [editMode, setEditMode] = useState<"rich" | "source">(() => readPref("lens.reader.editMode", "rich", (v) => v === "rich" || v === "source"));
  const richApi = useRef<WysiwygApi | null>(null);
  const [exporting, setExporting] = useState(false);
  const [studying, setStudying] = useState(false);
  const bodyRef = useRef<HTMLDivElement>(null);
  const index = useRef<WordIndex | null>(null);
  // 电子书：章节和目录，以及正在读的章节（anchor 是从目录或脚注跳过来要到的位置，否则回到 pos）
  const [book, setBook] = useState<BookInfo | null>(null);
  const [at, setAt] = useState<{ idx: number; pos: number; anchor?: string | null } | null>(null);
  const [tocOpen, setTocOpen] = useState(false);
  const [tocAt, setTocAt] = useState(-1);
  const tocRef = useRef<HTMLElement>(null);
  // 每排好一次版加一：接着扫描生词
  const [rendered, setRendered] = useState(0);
  const articleId = article?.id;

  // 读文章、排版、包词，然后恢复上次读到的位置
  useEffect(() => {
    let alive = true;
    setArticle(null);
    setScan(null);
    setBook(null);
    setAt(null);
    openArticle(id).then(
      (a) => {
        if (!alive) return;
        if (!a) setError(tr("这篇文章已被删除"));
        setArticle(a);
        if (a && a.chapters > 0) {
          openBook(id).then(
            (b) => {
              if (!alive || !b) return;
              setBook(b);
              setAt({ idx: Math.min(b.chapter, b.chapters.length - 1), pos: b.chapterPos });
            },
            (e) => alive && setError(String(e)),
          );
        }
      },
      (e) => alive && setError(String(e)),
    );
    return () => {
      alive = false;
    };
  }, [id]);

  useEffect(() => {
    const el = bodyRef.current;
    if (!article || !el || article.chapters > 0) return;
    el.innerHTML = renderMarkdown(article.body, imageLabel);
    index.current = wrapWords(el);
    setRendered((n) => n + 1);
    requestAnimationFrame(() => {
      const root = scrollRoot();
      root.scrollTo({ top: article.progress * Math.max(0, root.scrollHeight - root.clientHeight) });
    });
  }, [article]);

  // 电子书：换章时读这一章、排版，到目录指的位置或上次读到的位置
  useEffect(() => {
    const el = bodyRef.current;
    if (articleId === undefined || !el || !book || !at) return;
    let alive = true;
    setScan(null);
    const href = book.chapters[at.idx].href;
    bookChapter(articleId, at.idx).then(
      (raw) => {
        if (!alive) return;
        el.innerHTML = renderBookChapter(raw, href, articleId, imageLabel);
        index.current = wrapWords(el);
        setRendered((n) => n + 1);
        requestAnimationFrame(() => {
          const root = scrollRoot();
          if (!scrollToAnchor(el, at.anchor)) root.scrollTo({ top: at.pos * Math.max(0, root.scrollHeight - root.clientHeight) });
          setTocAt(tocIndexAt(book.toc, at.idx, el, root.getBoundingClientRect().top));
        });
      },
      (e) => alive && setError(String(e)),
    );
    return () => {
      alive = false;
    };
  }, [articleId, book, at]);

  // 跳到书签：同一章直接滚过去，别的章先换章再滚到位置
  const jumpTo = (chapter: number, pos: number) => {
    if (book && articleId !== undefined) {
      const idx = Math.min(Math.max(0, chapter), book.chapters.length - 1);
      if (idx !== at?.idx) {
        void setBookProgress(articleId, idx, pos).catch(() => undefined);
        setAt({ idx, pos });
        return;
      }
    }
    const root = scrollRoot();
    root.scrollTo({ top: pos * Math.max(0, root.scrollHeight - root.clientHeight) });
  };

  const goChapter = (idx: number, anchor?: string | null) => {
    setTocOpen(false);
    if (!book || articleId === undefined || idx < 0 || idx >= book.chapters.length) return;
    if (idx === at?.idx) {
      if (!scrollToAnchor(bodyRef.current, anchor)) scrollRoot().scrollTo({ top: 0 });
      return;
    }
    void setBookProgress(articleId, idx, 0).catch(() => undefined);
    setAt({ idx, pos: 0, anchor });
  };

  useEffect(() => {
    if (!tocOpen) return;
    const close = (e: Event) => {
      if (e instanceof KeyboardEvent ? e.key === "Escape" : !(e.target as HTMLElement).closest?.("[data-toc]")) setTocOpen(false);
    };
    // 打开时把当前项滚到中间（只滚目录自己，不动正文）
    const nav = tocRef.current;
    const current = nav?.querySelector<HTMLElement>("[aria-current]");
    if (nav && current) nav.scrollTop = current.offsetTop - nav.clientHeight / 2;
    window.addEventListener("keydown", close);
    window.addEventListener("mousedown", close);
    return () => {
      window.removeEventListener("keydown", close);
      window.removeEventListener("mousedown", close);
    };
  }, [tocOpen]);

  const rescan = useCallback(async () => {
    const ix = index.current;
    if (!ix) return;
    try {
      const r = await scanText(ix.plain, level);
      // 扫描期间换了章：这份结果对不上现在的正文
      if (index.current === ix) setScan(r);
    } catch (e) {
      setError(String(e));
    }
  }, [level]);

  useEffect(() => {
    if (rendered > 0) void rescan();
  }, [rendered, rescan]);

  // 扫描结果标到正文的词上
  useEffect(() => {
    const el = bodyRef.current;
    const ix = index.current;
    if (!el || !ix || !scan) return;
    for (const s of Array.from(el.querySelectorAll<HTMLElement>("[data-status]"))) {
      delete s.dataset.status;
      delete s.dataset.lemma;
    }
    for (const t of scan.tokens) {
      const span = ix.byStart.get(t.start);
      const w = scan.words[t.word];
      if (span && w) {
        span.dataset.status = w.status;
        span.dataset.lemma = w.lemma;
      }
    }
  }, [scan]);

  // 往下读时收起工具栏，让出版面；往上滑、回到顶部或鼠标移到最上面时再出来（编辑时一直显示）
  const [barHidden, setBarHidden] = useState(false);
  useEffect(() => {
    if (!article || draft !== null) {
      setBarHidden(false);
      return;
    }
    const root = scrollRoot();
    let last = root.scrollTop;
    const onScroll = () => {
      const top = root.scrollTop;
      const d = top - last;
      if (top < 60) setBarHidden(false);
      else if (d > 6) setBarHidden(true);
      else if (d < -6) setBarHidden(false);
      if (Math.abs(d) > 6) last = top;
    };
    const onMove = (e: MouseEvent) => {
      if (e.clientY - root.getBoundingClientRect().top < 48) setBarHidden(false);
    };
    root.addEventListener("scroll", onScroll, { passive: true });
    root.addEventListener("mousemove", onMove, { passive: true });
    return () => {
      root.removeEventListener("scroll", onScroll);
      root.removeEventListener("mousemove", onMove);
    };
  }, [article, draft]);

  // 记下读到哪里
  const chapterIdx = at?.idx;
  useEffect(() => {
    if (!article) return;
    const root = scrollRoot();
    let t: ReturnType<typeof setTimeout> | undefined;
    const onScroll = () => {
      clearTimeout(t);
      t = setTimeout(() => {
        const max = root.scrollHeight - root.clientHeight;
        if (max <= 0) return;
        const pos = root.scrollTop / max;
        const el = bodyRef.current;
        if (book && chapterIdx !== undefined && el) setTocAt(tocIndexAt(book.toc, chapterIdx, el, root.getBoundingClientRect().top));
        void (chapterIdx === undefined ? setArticleProgress(article.id, pos) : setBookProgress(article.id, chapterIdx, pos)).catch(() => undefined);
      }, 600);
    };
    root.addEventListener("scroll", onScroll, { passive: true });
    return () => {
      clearTimeout(t);
      root.removeEventListener("scroll", onScroll);
    };
  }, [article, book, chapterIdx]);

  // ── 点词查词 ──
  const lastPick = useRef<{ word: string; at: number } | null>(null);
  const pick = useCallback(
    async (word: string, range: Range, lemma: string | null = null) => {
      if (!article) return;
      const now = Date.now();
      // 双击时 click 和 mouseup 都会到，同一个词只查一次
      if (lastPick.current && lastPick.current.word === word && now - lastPick.current.at < 800) return;
      lastPick.current = { word, at: now };
      const para = paragraphOf(range);
      const contextId = para ? await saveReadingContext(word, para.paragraph, para.sel16, article.title).catch(() => null) : null;
      setPicked({ word, contextId, lemma, at: now });
      setTab("dict");
      // 右侧栏关着时点词，打开它显示释义
      if (!useReader.getState().sideHost) useSettings.getState().updateLayout({ showContext: true });
      void useLookup.getState().search(word, { commit: true, contextId });
    },
    [article],
  );

  const onClick = (e: React.MouseEvent) => {
    const t = e.target as HTMLElement;
    const a = t.closest("a");
    if (a) {
      e.preventDefault();
      const href = a.getAttribute("href") ?? "";
      if (/^https?:\/\//i.test(href)) void openUrl(href);
      else if (book && at && !/^[a-z][a-z0-9+.-]*:/i.test(href)) {
        // 书里的链接（脚注、目录页）：跳到那一章的那个位置
        const [path, anchor] = resolveBookPath(book.chapters[at.idx].href, href);
        const idx = book.chapters.findIndex((c) => c.href === path);
        if (idx >= 0) goChapter(idx, anchor);
      }
      return;
    }
    const sel = window.getSelection();
    if (sel && !sel.isCollapsed) return;
    const w = t.closest<HTMLElement>(`.${WORD_CLASS}`);
    if (!w?.textContent) return;
    const r = document.createRange();
    r.selectNodeContents(w);
    void pick(w.textContent, r, w.dataset.lemma ?? null);
  };

  // 拖选一个词或短语
  const onMouseUp = (e: React.MouseEvent) => {
    if (e.button !== 0 || e.detail > 1) return;
    setTimeout(() => {
      const sel = window.getSelection();
      if (!sel || sel.isCollapsed || sel.rangeCount === 0) return;
      const text = sel.toString().trim();
      if (acceptable(text, true)) void pick(text.replace(/\s+/g, " "), sel.getRangeAt(0));
    }, 0);
  };

  // 查到后自动收藏（带原句）
  const { handle, cards, done } = useLookup();
  const savedFor = useRef<number | null>(null);
  const [savedWords, setSavedWords] = useState<Set<string>>(new Set());
  const doSave = useCallback(
    async (p: Picked, quiet: boolean) => {
      try {
        // 简释优先用双解词典的中文（和生词扫描一致），没有再用首部词典的摘要
        const zh = await wordGlosses([p.word]).then((g) => g[0]?.gloss ?? null, () => null);
        await saveWord(p.word, { brief: zh ?? briefOf(useLookup.getState().cards, p.word), contextId: p.contextId });
        setSavedWords((prev) => new Set(prev).add(p.word.toLowerCase()));
        toast(quiet ? tr("已收进生词本：{w}（带原句）", { w: p.word }) : tr("已收藏「{q}」", { q: p.word }));
        void rescan();
      } catch (e) {
        toast(tr("收藏失败：{error}", { error: String(e) }));
      }
    },
    [rescan],
  );
  useEffect(() => {
    if (!picked || !autoSave || savedFor.current === picked.at) return;
    if (!done || !handle || handle.query.trim() !== picked.word) return;
    if (!cards.some((c) => c.status === "hit")) return;
    savedFor.current = picked.at;
    void doSave(picked, true);
  }, [picked, autoSave, done, handle, cards, doSave]);

  // 在右栏改查别的词（改输入框、点「查原形」）：仍算同一处，收藏时带着原句
  const requery = (w: string) => {
    const word = w.trim();
    if (!word) return;
    const contextId = picked?.contextId ?? null;
    setPicked({ word, contextId, lemma: null, at: Date.now() });
    savedFor.current = null;
    void useLookup.getState().search(word, { commit: true, contextId });
  };

  const lookupInPanel = (w: string) => {
    setPicked({ word: w, contextId: null, at: Date.now() });
    savedFor.current = null;
    setTab("dict");
    if (!useReader.getState().sideHost) useSettings.getState().updateLayout({ showContext: true });
    void useLookup.getState().search(w, { commit: true });
  };

  const locate = (lemma: string) => {
    const el = bodyRef.current?.querySelector<HTMLElement>(`[data-lemma="${CSS.escape(lemma)}"]`);
    if (!el) return;
    el.scrollIntoView({ block: "center", behavior: "smooth" });
    el.dataset.flash = "";
    setTimeout(() => delete el.dataset.flash, 1600);
  };

  const saveTitle = async (t: string) => {
    setEditingTitle(false);
    if (!article || !t.trim() || t.trim() === article.title) return;
    try {
      await renameArticle(article.id, t);
      setArticle({ ...article, title: t.trim() });
    } catch (e) {
      setError(String(e));
    }
  };

  // ── 编辑正文 ──
  const editAt = useRef(0);
  const startEdit = () => {
    if (!article) return;
    const root = scrollRoot();
    const max = root.scrollHeight - root.clientHeight;
    editAt.current = max > 0 ? root.scrollTop / max : 0;
    setConfirmDiscard(false);
    setDraft(article.body);
    requestAnimationFrame(() => root.scrollTo({ top: 0 }));
  };
  // 当前编辑的内容：所见即所得模式下直接从编辑器取（它的回传有防抖）
  const currentText = () => (draft === null ? null : editMode === "rich" ? (richApi.current?.markdown() ?? draft) : draft);
  const isDirty = () => {
    const t = currentText();
    return t !== null && article !== null && t !== article.body;
  };
  const switchEditMode = (m: "rich" | "source") => {
    if (m === editMode) return;
    if (editMode === "rich") setDraft(currentText());
    setEditMode(m);
    writePref("lens.reader.editMode", m);
  };
  // 双击正文的空白处进入编辑（不放「编辑」按钮）：双击在词、链接、控件上，或双击选中了文字时不算
  const onBlankDoubleClick = (e: React.MouseEvent) => {
    if (draft !== null || !article || article.chapters > 0) return;
    const t = e.target as HTMLElement;
    if (t.closest(`header, button, input, select, textarea, a, label, .${WORD_CLASS}`)) return;
    if (onText(e.clientX, e.clientY)) return;
    window.getSelection()?.removeAllRanges();
    startEdit();
  };
  const cancelEdit = () => {
    if (isDirty() && !confirmDiscard) {
      setConfirmDiscard(true);
      return;
    }
    setDraft(null);
    setConfirmDiscard(false);
    // 正文没变，回到进编辑前的位置
    requestAnimationFrame(() => {
      const root = scrollRoot();
      root.scrollTo({ top: editAt.current * Math.max(0, root.scrollHeight - root.clientHeight) });
    });
  };
  const saveBody = async () => {
    const text = currentText();
    if (!article || text === null || savingBody) return;
    if (text === article.body) {
      cancelEdit();
      return;
    }
    if (!text.trim()) {
      toast(tr("保存失败：{error}", { error: tr("文章是空的") }));
      return;
    }
    setSavingBody(true);
    try {
      const words = await updateArticleBody(article.id, text);
      // 换了 article 对象就会重新排版、重新扫描，并滚回进编辑前的位置
      setArticle({ ...article, body: text, words, progress: editAt.current });
      setDraft(null);
      setConfirmDiscard(false);
      toast(tr("已保存"));
    } catch (e) {
      toast(tr("保存失败：{error}", { error: String(e) }));
    } finally {
      setSavingBody(false);
    }
  };

  const sideHost = useReader((s) => s.sideHost);
  const chapterInfo = book && at ? book.chapters[at.idx] : null;
  // 工具栏显示正在读的那一节（一个文件可能分成好几节，切到别的章时 tocAt 还没更新就先用章节名）
  const tocItem = book && at && book.toc[tocAt]?.chapter === at.idx ? book.toc[tocAt] : null;
  // 导出的正文：图片换成说明，离开 Lens 就打不开
  const exportHtml = () => {
    const el = bodyRef.current;
    if (!el) return "";
    if (!book) return el.innerHTML;
    const copy = el.cloneNode(true) as HTMLElement;
    for (const img of Array.from(copy.querySelectorAll("img"))) {
      const span = document.createElement("span");
      span.className = "lens-img";
      span.textContent = imageLabel(img.getAttribute("alt") ?? "");
      img.replaceWith(span);
    }
    return copy.innerHTML;
  };

  const toggle = (key: string, v: boolean, set: (v: boolean) => void) => {
    set(v);
    writePref(key, v);
  };

  return (
    <div className={styles.wide}>
      <header className={styles.readerBar} data-hidden={(barHidden && !tocOpen) || undefined} onFocus={() => setBarHidden(false)}>
        <button type="button" className={styles.ghost} onClick={onBack} disabled={draft !== null}>
          {tr("← 文章库")}
        </button>
        {editingTitle && article ? (
          <input
            className={styles.titleInput}
            defaultValue={article.title}
            autoFocus
            aria-label={tr("文章标题")}
            onBlur={(e) => void saveTitle(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void saveTitle(e.currentTarget.value);
              if (e.key === "Escape") setEditingTitle(false);
            }}
          />
        ) : (
          <button type="button" className={styles.titleBtn} data-book={book ? "" : undefined} title={tr("改标题")} onClick={() => setEditingTitle(true)} disabled={!article}>
            {article?.title ?? "…"}
          </button>
        )}
        {chapterInfo && book && at && (
          <span className={styles.tocWrap} data-toc="">
            <button
              type="button"
              className={styles.chapterBtn}
              data-hint="toc"
              aria-expanded={tocOpen}
              title={tr("目录")}
              onClick={() => {
                markHint("toc");
                setTocOpen((o) => !o);
              }}
            >
              {tocItem?.title ?? chapterInfo.title}
            </button>
            <Hint id="toc" anchor="toc" show={!tocOpen && book.toc.length > 1}>
              {tr("点章节名打开目录，可以跳到任意一章。")}
            </Hint>
            {tocOpen && (
              <nav ref={tocRef} className={styles.toc} aria-label={tr("目录")}>
                {book.toc.map((t, i) => (
                  <button
                    key={`${i}-${t.chapter}`}
                    type="button"
                    className={styles.tocItem}
                    style={{ paddingLeft: 10 + t.level * 16 }}
                    aria-current={i === tocAt || undefined}
                    onClick={() => goChapter(t.chapter, t.anchor)}
                  >
                    {t.title}
                  </button>
                ))}
              </nav>
            )}
          </span>
        )}
        <span className={styles.barSpacer} />
        {draft !== null ? (
          <>
            <div role="group" aria-label={tr("编辑方式")} className={styles.segmentedSmall}>
              <button type="button" aria-pressed={editMode === "rich"} onClick={() => switchEditMode("rich")} title={tr("在排好版的正文上直接改")}>
                {tr("所见即所得")}
              </button>
              <button type="button" aria-pressed={editMode === "source"} onClick={() => switchEditMode("source")} title={tr("改 Markdown 原文")}>
                {tr("源码")}
              </button>
            </div>
            <span className={styles.faint}>{tr("Ctrl+S 保存")}</span>
            <button type="button" className={confirmDiscard ? styles.danger : styles.ghost} onClick={cancelEdit}>
              {confirmDiscard ? tr("放弃修改？") : tr("取消")}
            </button>
            <button type="button" className={styles.primarySmall} onClick={() => void saveBody()} disabled={savingBody}>
              {tr("保存")}
            </button>
          </>
        ) : (
          <>
            {article && (
              <Bookmarks
                articleId={article.id}
                chapter={at?.idx ?? 0}
                chapterTitles={book ? book.chapters.map((c) => c.title) : null}
                body={bodyRef}
                onJump={jumpTo}
              />
            )}
            <button type="button" className={styles.ghost} onClick={() => setExporting(true)} disabled={!article || !scan} title={tr("导出 PDF / HTML：标注过的原文 + 词汇表")}>
              {tr("导出")}
            </button>
            <label className={styles.inline}>
              {tr("词汇量")}
              <select
                value={level}
                onChange={(e) => {
                  const v = Number(e.target.value);
                  setLevel(v);
                  writePref("lens.scan.level", v);
                }}
              >
                {LEVELS.map((l) => (
                  <option key={l} value={l}>
                    {tr("最常用的 {n} 词", { n: l })}
                  </option>
                ))}
              </select>
            </label>
            <label className={styles.inline}>
              <input type="checkbox" checked={marks} onChange={(e) => toggle("lens.reader.marks", e.target.checked, setMarks)} />
              {tr("标出生词")}
            </label>
            <label className={styles.inline} title={tr("点词查到后，连同所在句子收进生词本")}>
              <input type="checkbox" checked={autoSave} onChange={(e) => toggle("lens.reader.autoSave", e.target.checked, setAutoSave)} />
              {tr("查词自动收藏")}
            </label>
          </>
        )}
      </header>

      {error && (
        <p role="alert" className={styles.error}>
          {error}
        </p>
      )}

      <div className={styles.paper} onDoubleClick={onBlankDoubleClick}>
        {draft !== null && (
          <>
            {article?.sourcePath && <p className={styles.faint}>{tr("只改 Lens 里的这份，不会改动原文件。")}</p>}
            {editMode === "rich" ? (
              <WysiwygEditor
                initial={draft}
                api={richApi}
                onChange={(md) => {
                  setDraft(md);
                  setConfirmDiscard(false);
                }}
                onSave={() => void saveBody()}
              />
            ) : (
              <textarea
                className={styles.editor}
                value={draft}
                autoFocus
                spellCheck={false}
                aria-label={tr("Markdown 原文")}
                onChange={(e) => {
                  setDraft(e.target.value);
                  setConfirmDiscard(false);
                }}
                onKeyDown={(e) => {
                  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
                    e.preventDefault();
                    e.stopPropagation();
                    void saveBody();
                  }
                }}
              />
            )}
          </>
        )}
        <div
          hidden={draft !== null}
          ref={bodyRef}
          className={styles.prose}
          data-reader=""
          data-marks={marks || undefined}
          onClick={onClick}
          onMouseUp={onMouseUp}
        />
        {book && at && draft === null && (
          <nav className={styles.chapterNav} aria-label={tr("翻章")}>
            <button type="button" className={styles.ghost} disabled={at.idx === 0} onClick={() => goChapter(at.idx - 1)}>
              {tr("← 上一章")}
            </button>
            <span className={styles.faint}>{`${at.idx + 1} / ${book.chapters.length}`}</span>
            <button type="button" className={styles.ghost} disabled={at.idx >= book.chapters.length - 1} onClick={() => goChapter(at.idx + 1)}>
              {tr("下一章 →")}
            </button>
          </nav>
        )}
        {article && draft === null && <p className={styles.faint}>{tr("点一个词查词，拖选可以查短语。")}</p>}
      </div>

      <ArticleExportDialog
        open={exporting}
        onClose={() => setExporting(false)}
        title={chapterInfo ? `${article?.title ?? ""} · ${chapterInfo.title}` : (article?.title ?? "")}
        scan={scan}
        getHtml={exportHtml}
        onStudy={
          book && article
            ? () => {
                setExporting(false);
                setStudying(true);
              }
            : undefined
        }
      />
      {book && article && <BookStudyDialog open={studying} onClose={() => setStudying(false)} bookId={article.id} level={level} />}

      {sideHost &&
        createPortal(
          draft !== null ? (
            // 编辑时生词表对不上正文，保存后重新统计
            <p className={styles.sideEmpty}>{tr("正在编辑正文，保存后重新统计生词。")}</p>
          ) : (
            <section className={styles.panel} aria-label={tr("释义与生词")}>
              <div role="tablist" aria-label={tr("右侧栏")} className={styles.segmented}>
                <button type="button" role="tab" aria-selected={tab === "dict"} aria-pressed={tab === "dict"} onClick={() => setTab("dict")}>
                  {tr("释义")}
                </button>
                <button type="button" role="tab" aria-selected={tab === "words"} aria-pressed={tab === "words"} onClick={() => setTab("words")}>
                  {tr("生词")} {scan ? scan.words.filter((w) => w.status === "new").length : ""}
                </button>
              </div>
              {tab === "dict" ? (
                <DictPanel
                  picked={picked}
                  autoSave={autoSave}
                  savedWords={savedWords}
                  onSave={(p) => void doSave(p, false)}
                  onLookup={lookupInPanel}
                  onRequery={requery}
                />
              ) : (
                <WordsPanel
                  scan={scan}
                  title={article?.title ?? ""}
                  onLocate={locate}
                  onLookup={lookupInPanel}
                  onChanged={() => void rescan()}
                  onError={setError}
                />
              )}
            </section>
          ),
          sideHost,
        )}
    </div>
  );
}

// ── 右侧栏：释义 ──

function DictPanel({
  picked,
  autoSave,
  savedWords,
  onSave,
  onLookup,
  onRequery,
}: {
  picked: Picked | null;
  autoSave: boolean;
  savedWords: Set<string>;
  onSave: (p: Picked) => void;
  onLookup: (w: string) => void;
  onRequery: (w: string) => void;
}) {
  const { handle, cards, done } = useLookup();
  const copyCfg = useSettings((s) => s.settings?.copy) ?? DEFAULT_COPY;
  const updateSettings = useSettings((s) => s.update);
  const [gloss, setGloss] = useState<WordGloss | null>(null);
  const [inBook, setInBook] = useState(false);
  const word = handle?.query.trim() ?? "";
  // 词头可以直接改：改完回车重新查
  const [q, setQ] = useState(word);
  useEffect(() => setQ(word), [word]);
  const submitQuery = () => {
    const t = q.trim();
    if (t && t !== word) onRequery(t);
  };

  useEffect(() => {
    setGloss(null);
    setInBook(false);
    if (!word) return;
    let alive = true;
    wordGlosses([word]).then((g) => alive && setGloss(g[0] ?? null), () => undefined);
    wordId(word).then((id) => alive && setInBook(id !== null), () => undefined);
    return () => {
      alive = false;
    };
  }, [word, savedWords]);

  if (!picked || !handle) {
    return <p className={styles.empty}>{tr("点文章里的词，释义显示在这里。")}</p>;
  }
  // 词典的内容和顺序跟查词页一样，按当前场景配置组（设置 › 词典里调）
  const hits = cards.filter((c) => c.status === "hit" || c.status === "pending");
  const isPicked = picked.word === word;

  return (
    <div className={styles.dictPanel}>
      <div className={styles.dictHead}>
        <form
          className={styles.queryForm}
          onSubmit={(e) => {
            e.preventDefault();
            submitQuery();
          }}
        >
          <input
            className={styles.queryInput}
            value={q}
            size={Math.max(4, q.length + 1)}
            spellCheck={false}
            aria-label={tr("查的词（可以改，回车重新查）")}
            title={tr("可以改，回车重新查")}
            onChange={(e) => setQ(e.target.value)}
            onKeyDown={(e) => {
              // 回车直接在这里处理：不依赖表单的隐式提交（别处的全局按键处理可能先拦下）
              if (e.key === "Enter" && !e.nativeEvent.isComposing) {
                e.preventDefault();
                submitQuery();
              } else if (e.key === "Escape") setQ(word);
            }}
          />
        </form>
        {gloss?.pos && <span className={styles.pos}>{gloss.pos}</span>}
        {picked.lemma && isPicked && picked.lemma !== word.toLowerCase() && (
          <button type="button" className={styles.lemmaBtn} onClick={() => onRequery(picked.lemma ?? word)} title={tr("查这个词的原形")}>
            {tr("查原形 {w}", { w: picked.lemma })}
          </button>
        )}
        <span className={styles.barSpacer} />
        {inBook ? (
          <span className={styles.savedTag}>{tr("已在生词本")}</span>
        ) : (
          <button type="button" className={styles.primarySmall} disabled={!done || hits.length === 0} onClick={() => onSave(isPicked ? picked : { word, contextId: null, at: Date.now() })}>
            {tr("收藏")}
          </button>
        )}
        <button type="button" className={styles.link} onClick={() => useRoute.getState().go("lookup")}>
          {tr("在查词页打开")}
        </button>
      </div>
      {gloss?.gloss && <p className={styles.glossLine}>{gloss.gloss}</p>}
      {!autoSave && isPicked && picked.contextId !== null && !inBook && <p className={styles.faint}>{tr("收藏时会带上这句原文。")}</p>}
      {done && hits.length === 0 && <p className={styles.empty}>{tr("本地词典里没有「{q}」", { q: word })}</p>}
      <div className={styles.cards}>
        {hits.map((c) => (
          <DictCard
            key={c.id}
            card={c}
            onEntry={onLookup}
            onToggleKeyDown={() => undefined}
            copyFormat={copyCfg.defaultFormat}
            onCopy={(f) => void copyCard(c, f, copyCfg)}
            onSetDefaultFormat={(f: CopyFormat) => {
              updateSettings({ copy: { defaultFormat: f } });
              toast(tr("默认复制格式：{format}", { format: tr(FORMAT_NAME[f]) }));
            }}
            onQueryOnline={() => void useLookup.getState().queryOnline(c.id)}
          />
        ))}
      </div>
    </div>
  );
}

// ── 右侧栏：生词 ──

function WordsPanel({
  scan,
  title,
  onLocate,
  onLookup,
  onChanged,
  onError,
}: {
  scan: ScanResult | null;
  title: string;
  onLocate: (lemma: string) => void;
  onLookup: (w: string) => void;
  onChanged: () => void;
  onError: (e: string) => void;
}) {
  const [group, setGroup] = useState<ScanStatus>("new");
  const [sort, setSort] = useState<"order" | "rank">("order");
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [glosses, setGlosses] = useState<Map<string, WordGloss>>(new Map());

  useEffect(() => {
    setPicked(new Set((scan?.words ?? []).filter((w) => w.status === "new").map((w) => w.lemma)));
  }, [scan]);

  useEffect(() => {
    if (!scan) return;
    let alive = true;
    const need = scan.words.map((w) => w.lemma).filter((l) => !glosses.has(l));
    void (async () => {
      for (let i = 0; i < need.length; i += GLOSS_BATCH) {
        const batch = await wordGlosses(need.slice(i, i + GLOSS_BATCH)).catch(() => []);
        if (!alive) return;
        setGlosses((prev) => {
          const next = new Map(prev);
          for (const g of batch) next.set(g.word, g);
          return next;
        });
      }
    })();
    return () => {
      alive = false;
    };
    // glosses 不进依赖：只在扫描结果变化时补取
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scan]);

  const counts = useMemo(() => {
    const c = { new: 0, saved: 0, known: 0 };
    for (const w of scan?.words ?? []) c[w.status] += 1;
    return c;
  }, [scan]);

  const shown = useMemo(() => {
    const list = (scan?.words ?? []).filter((w) => w.status === group);
    return sort === "rank" ? [...list].sort((a, b) => (b.rank ?? 1e9) - (a.rank ?? 1e9)) : list;
  }, [scan, group, sort]);

  if (!scan) return <p className={styles.empty}>{tr("扫描中…")}</p>;

  const pickedNew = shown.filter((w) => picked.has(w.lemma));

  const markKnown = async (lemmas: string[], known: boolean) => {
    try {
      await setKnownWords(lemmas, known);
      onChanged();
    } catch (e) {
      onError(String(e));
    }
  };

  // 导入用户认识的词表：都标成「认识」，生词标得更准
  const importKnown = async () => {
    const path = await openDialog({
      multiple: false,
      title: tr("选择认识的词表"),
      filters: [{ name: tr("词表"), extensions: ["txt", "csv", "tsv", "md"] }],
    });
    if (typeof path !== "string") return;
    try {
      const r = await importKnownWords(path);
      toast(tr("认出 {found} 个词，新增 {added} 个「认识」（共 {total} 个）", { found: r.found, added: r.added, total: r.total }));
      onChanged();
    } catch (e) {
      onError(String(e));
    }
  };
  const [confirmClear, setConfirmClear] = useState(false);
  const clearKnown = async () => {
    if (!confirmClear) {
      setConfirmClear(true);
      return;
    }
    setConfirmClear(false);
    try {
      await clearKnownWords();
      toast(tr("已清空「认识」的词"));
      onChanged();
    } catch (e) {
      onError(String(e));
    }
  };

  const savePicked = async (words: ScanWord[]) => {
    try {
      const n = await saveScannedWords(
        words.map((w) => ({
          lemma: w.lemma,
          surface: w.forms[0] ?? w.lemma,
          sentence: w.sentence,
          hlStart: w.hlStart,
          hlEnd: w.hlEnd,
          brief: glosses.get(w.lemma)?.gloss ?? null,
        })),
        title,
      );
      toast(tr("已加入生词本：{n} 个词", { n }));
      onChanged();
    } catch (e) {
      onError(String(e));
    }
  };

  return (
    <div className={styles.wordsPanel}>
      <p className={styles.stats}>
        {tr("{total} 词 · {unique} 个不同的词 · 生词 {n} 个 · 认识 {pct}%", {
          total: scan.total,
          unique: scan.unique,
          n: counts.new,
          pct: Math.round(scan.coverage * 100),
        })}
      </p>
      <div className={styles.panelHead}>
        <div role="group" aria-label={tr("分组")} className={styles.segmentedSmall}>
          {(["new", "saved", "known"] as const).map((s) => (
            <button key={s} type="button" aria-pressed={group === s} onClick={() => setGroup(s)}>
              {s === "new" ? tr("生词") : s === "saved" ? tr("已收藏") : tr("认识")} {counts[s]}
            </button>
          ))}
        </div>
        <button type="button" className={styles.link} onClick={() => setSort(sort === "order" ? "rank" : "order")}>
          {sort === "order" ? tr("按出现顺序") : tr("难的在前")}
        </button>
      </div>
      <div className={styles.knownBar}>
        <button
          type="button"
          className={styles.link}
          onClick={() => void importKnown()}
          title={tr("txt / csv，每行一个词（后面带释义、序号也行）；词表里的词都标成「认识」，不再算生词")}
        >
          {tr("导入认识的词表…")}
        </button>
        {group === "known" && counts.known > 0 && (
          <button type="button" className={confirmClear ? styles.danger : styles.link} onClick={() => void clearKnown()} onBlur={() => setConfirmClear(false)}>
            {confirmClear ? tr("确定清空全部「认识」？") : tr("清空「认识」")}
          </button>
        )}
      </div>

      {group === "new" && shown.length > 0 && (
        <div className={styles.bulk}>
          <label className={styles.check}>
            <input
              type="checkbox"
              checked={pickedNew.length === shown.length}
              ref={(el) => {
                if (el) el.indeterminate = pickedNew.length > 0 && pickedNew.length < shown.length;
              }}
              onChange={(e) => setPicked(e.target.checked ? new Set(shown.map((w) => w.lemma)) : new Set())}
            />
            {tr("全选")}
          </label>
          <button type="button" className={styles.primarySmall} disabled={pickedNew.length === 0} onClick={() => void savePicked(pickedNew)}>
            {tr("加入生词本（{n}）", { n: pickedNew.length })}
          </button>
          <button type="button" className={styles.ghost} disabled={pickedNew.length === 0} onClick={() => void markKnown(pickedNew.map((w) => w.lemma), true)}>
            {tr("标为认识")}
          </button>
        </div>
      )}

      {shown.length === 0 ? (
        <p className={styles.empty}>{group === "new" ? tr("没有超过词汇量的生词。") : tr("这一组是空的。")}</p>
      ) : (
        <ul className={styles.list}>
          {shown.map((w) => {
            const g = glosses.get(w.lemma);
            return (
              <li key={w.lemma} className={styles.row}>
                {group === "new" && (
                  <input
                    type="checkbox"
                    aria-label={tr("选中 {w}", { w: w.lemma })}
                    checked={picked.has(w.lemma)}
                    onChange={(e) =>
                      setPicked((prev) => {
                        const next = new Set(prev);
                        if (e.target.checked) next.add(w.lemma);
                        else next.delete(w.lemma);
                        return next;
                      })
                    }
                  />
                )}
                <div className={styles.rowMain}>
                  <div className={styles.rowLine}>
                    <button type="button" className={styles.word} title={tr("在文中找到它")} onClick={() => onLocate(w.lemma)}>
                      {w.lemma}
                    </button>
                    {g?.pos && <span className={styles.pos}>{g.pos}</span>}
                    {w.count > 1 && <span className={styles.faint}>×{w.count}</span>}
                  </div>
                  <div className={styles.gloss} title={g?.dict ? tr("中文释义取自《{d}》", { d: g.dict }) : g ? tr("当前配置组的本地词典里没有中文释义") : undefined}>
                    {g ? (g.gloss ?? "—") : "…"}
                  </div>
                </div>
                <span className={styles.rank} title={scan.source === "coca" ? tr("COCA 词频排名（数字越大越少见）") : tr("内置词频表排名（数字越大越少见；导入 COCA 后改用 COCA）")}>
                  {w.rank ? `#${w.rank}` : tr("罕见")}
                </span>
                <button type="button" className={styles.link} onClick={() => onLookup(w.lemma)}>
                  {tr("查")}
                </button>
                {w.status === "new" && (
                  <button type="button" className={styles.link} onClick={() => void markKnown([w.lemma], true)}>
                    {tr("认识")}
                  </button>
                )}
                {w.status === "known" && (
                  <button type="button" className={styles.link} onClick={() => void markKnown([w.lemma], false)}>
                    {tr("撤销")}
                  </button>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
