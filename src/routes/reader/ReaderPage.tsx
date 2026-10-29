// 阅读：文章库（粘贴 Markdown、打开或拖入 .md / .txt 文件）→ 阅读视图（ArticleView）。

import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { createArticle, deleteArticle, importArticleFiles, listArticles, type ArticleMeta } from "../../ipc/articles";
import { useReader } from "../../stores/reader";
import { toast } from "../../stores/toast";
import { ArticleView } from "./ArticleView";
import styles from "./ReaderPage.module.css";
import { getLang, tr } from "../../i18n";

const EXTENSIONS = ["md", "markdown", "txt"];

function date(ms: number): string {
  const d = new Date(ms);
  if (getLang() === "en") return d.toLocaleDateString("en", { month: "short", day: "numeric", year: "numeric" });
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

export function ReaderPage() {
  const openId = useReader((s) => s.openId);
  const open = useReader((s) => s.open);
  const rev = useReader((s) => s.rev);
  const [list, setList] = useState<ArticleMeta[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pasting, setPasting] = useState(false);
  const [draft, setDraft] = useState({ title: "", body: "" });
  const [confirmDelete, setConfirmDelete] = useState<number | null>(null);

  const refresh = useCallback(() => {
    listArticles().then(setList, (e) => setError(String(e)));
  }, []);

  useEffect(() => {
    if (openId === null) refresh();
  }, [openId, refresh]);

  const importPaths = useCallback(
    async (paths: string[]) => {
      const files = paths.filter((p) => EXTENSIONS.includes(p.split(".").pop()?.toLowerCase() ?? ""));
      if (files.length === 0) {
        toast(tr("只支持 .md 和 .txt 文件"));
        return;
      }
      try {
        const r = await importArticleFiles(files);
        for (const [file, why] of r.failed) toast(tr("没有导入 {file}：{why}", { file: file.split(/[\\/]/).pop() ?? file, why }));
        if (r.ids.length === 1) open(r.ids[0]);
        else if (r.ids.length > 1) {
          toast(tr("已导入 {n} 篇文章", { n: r.ids.length }));
          refresh();
        }
      } catch (e) {
        setError(String(e));
      }
    },
    [open, refresh],
  );

  // 把文件拖进窗口
  useEffect(() => {
    const off = getCurrentWebview().onDragDropEvent((e) => {
      if (e.payload.type === "drop") void importPaths(e.payload.paths);
    });
    return () => void off.then((f) => f());
  }, [importPaths]);

  const pickFiles = async () => {
    const picked = await openDialog({ multiple: true, title: tr("打开文章"), filters: [{ name: tr("Markdown / 文本"), extensions: EXTENSIONS }] });
    if (!picked) return;
    await importPaths(Array.isArray(picked) ? picked : [picked]);
  };

  const savePaste = async () => {
    try {
      const id = await createArticle(draft.body, draft.title);
      setDraft({ title: "", body: "" });
      setPasting(false);
      open(id);
    } catch (e) {
      setError(String(e));
    }
  };

  const remove = async (id: number) => {
    try {
      await deleteArticle(id);
      setConfirmDelete(null);
      refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  const sideHost = useReader((s) => s.sideHost);

  if (openId !== null) return <ArticleView key={`${openId}-${rev}`} id={openId} onBack={() => open(null)} />;

  return (
    <div className={styles.column}>
      {sideHost && createPortal(<p className={styles.sideEmpty}>{tr("打开一篇文章，点词后释义显示在这里。")}</p>, sideHost)}
      <header className={styles.head}>
        <h1 className={styles.title}>{tr("阅读")}</h1>
        <span className={styles.barSpacer} />
        <button type="button" className={styles.ghost} onClick={() => void pickFiles()}>
          {tr("打开文件…")}
        </button>
        <button type="button" className={styles.primary} onClick={() => setPasting((p) => !p)} aria-expanded={pasting}>
          {tr("粘贴新文章")}
        </button>
      </header>
      <p className={styles.sub}>{tr("把英文文章存进 Lens 来读：点词就在右边查词，查过的词连同所在句子自动收进生词本；生词按词频标在正文上。支持 Markdown，也可以把 .md / .txt 文件拖进窗口。")}</p>

      {error && (
        <p role="alert" className={styles.error}>
          {error}
        </p>
      )}

      {pasting && (
        <section className={styles.pasteBox} aria-label={tr("粘贴新文章")}>
          <label className={styles.field}>
            <span>{tr("标题（可选，不填就用第一个 # 标题或第一行）")}</span>
            <input type="text" value={draft.title} onChange={(e) => setDraft({ ...draft, title: e.target.value })} />
          </label>
          <label className={styles.field}>
            <span className="sr-only">{tr("文章")}</span>
            <textarea className={styles.paste} value={draft.body} onChange={(e) => setDraft({ ...draft, body: e.target.value })} placeholder={tr("在这里粘贴 Markdown 或纯文本…")} spellCheck={false} autoFocus />
          </label>
          <div className={styles.bar}>
            <button type="button" className={styles.ghost} onClick={() => setPasting(false)}>
              {tr("取消")}
            </button>
            <button type="button" className={styles.primary} disabled={!draft.body.trim()} onClick={() => void savePaste()}>
              {tr("保存并阅读")}
            </button>
          </div>
        </section>
      )}

      {list && list.length === 0 && !pasting && <p className={styles.empty}>{tr("还没有文章。粘贴一篇，或打开 .md / .txt 文件。")}</p>}

      {list && list.length > 0 && (
        <ul className={styles.library}>
          {list.map((a) => (
            <li key={a.id} className={styles.item}>
              <button type="button" className={styles.itemMain} onClick={() => open(a.id)}>
                <span className={styles.itemTitle}>{a.title}</span>
                {a.excerpt && <span className={styles.itemExcerpt}>{a.excerpt}</span>}
                <span className={styles.itemMeta}>
                  {tr("{n} 词", { n: a.words })}
                  {a.progress > 0.02 && ` · ${tr("读到 {p}%", { p: Math.round(a.progress * 100) })}`}
                  {` · ${date(a.openedAt ?? a.createdAt)}`}
                  {a.sourcePath && ` · ${a.sourcePath.split(/[\\/]/).pop()}`}
                </span>
              </button>
              {confirmDelete === a.id ? (
                <span className={styles.confirm}>
                  <button type="button" className={styles.danger} onClick={() => void remove(a.id)}>
                    {tr("确认删除")}
                  </button>
                  <button type="button" className={styles.ghost} onClick={() => setConfirmDelete(null)}>
                    {tr("取消")}
                  </button>
                </span>
              ) : (
                <button type="button" className={styles.remove} aria-label={tr("删除「{t}」", { t: a.title })} onClick={() => setConfirmDelete(a.id)}>
                  ×
                </button>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
