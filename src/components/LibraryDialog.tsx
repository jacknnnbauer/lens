// 词典库（简版）：已导入词典列表、导入（文件对话框）、导入进度、删除；在线词典的添加与设置（M6）。
// 完整的词典库页（分组、排序、配置组）见画板 03，后续里程碑实现。

import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { useEffect, useRef, useState } from "react";
import { fileName, useDicts } from "../stores/dicts";
import { CloseIcon, GripIcon, PlusIcon } from "./icons";
import { useDragReorder } from "./useDragReorder";
import styles from "./LibraryDialog.module.css";
import { AddOnlinePanel, OnlineSettings, onlineMeta } from "./OnlineDicts";
import { tr } from "../i18n";

const KIND_LABEL: Record<string, string> = { mdx: "MDict", stardict: "StarDict", json: "JSON", online: "在线" };

interface Props {
  open: boolean;
  onClose: () => void;
}

export function LibraryDialog({ open, onClose }: Props) {
  const dialog = useRef<HTMLDialogElement>(null);
  const { dicts, imports, error, importPaths, remove, move, setOrder } = useDicts();
  const drag = useDragReorder(
    dicts.map((d) => d.id),
    (ids) => void setOrder(ids),
  );
  const [confirming, setConfirming] = useState<string | null>(null);
  const [adding, setAdding] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState<string | null>(null);

  useEffect(() => {
    const d = dialog.current;
    if (!d) return;
    if (open && !d.open) d.showModal();
    if (!open && d.open) d.close();
  }, [open]);

  const pick = async () => {
    const picked = await openFileDialog({
      multiple: true,
      title: tr("选择词典文件"),
      filters: [{ name: tr("词典（.mdx / .ifo / .json）"), extensions: ["mdx", "ifo", "json"] }],
    });
    if (picked) await importPaths(Array.isArray(picked) ? picked : [picked]);
  };

  const running = Object.values(imports).filter((p) => p.phase === "start");
  const failed = Object.values(imports).filter((p) => p.phase === "error");

  return (
    <dialog ref={dialog} className={styles.dialog} onClose={onClose} aria-labelledby="library-title">
      <header className={styles.head}>
        <h2 id="library-title" className={styles.title}>
          {tr("词典库")}
        </h2>
        <button type="button" className={styles.ghost} aria-expanded={adding} onClick={() => setAdding((a) => !a)}>
          {tr("添加在线词典")}
        </button>
        <button type="button" className={styles.primary} onClick={() => void pick()}>
          <PlusIcon />
          {tr("导入词典…")}
        </button>
        <button type="button" className={styles.iconBtn} aria-label={tr("关闭")} onClick={onClose}>
          <CloseIcon />
        </button>
      </header>

      {adding && <AddOnlinePanel onDone={() => setAdding(false)} />}

      {(running.length > 0 || failed.length > 0) && (
        <ul className={styles.progress} aria-live="polite">
          {running.map((p) => (
            <li key={p.path}>
              <span className={styles.spinner} aria-hidden="true" />
              {tr("正在建索引：{file}（{i}/{n}）", { file: fileName(p.path), i: p.index + 1, n: p.total })}
            </li>
          ))}
          {failed.map((p) => (
            <li key={p.path} className={styles.failed}>
              {fileName(p.path)}: {p.error}
            </li>
          ))}
        </ul>
      )}
      {error && <p className={styles.failed}>{error}</p>}

      {dicts.length === 0 ? (
        <p className={styles.empty}>{tr("还没有导入词典。支持 MDict（.mdx，同目录的 .mdd 自动带上）、StarDict（.ifo）和 Lens JSON。")}</p>
      ) : (
        <ul className={styles.list}>
          {dicts.map((d, index) => (
            <li
              key={d.id}
              ref={drag.rowRef(d.id)}
              className={styles.item}
              {...drag.rowState(d.id, index)}
              onKeyDown={(e) => {
                // Alt+↑ / Alt+↓ 移动（画板 03）
                if (!e.altKey || (e.key !== "ArrowUp" && e.key !== "ArrowDown")) return;
                e.preventDefault();
                void move(d.id, e.key === "ArrowUp" ? -1 : 1).then(() =>
                  requestAnimationFrame(() => document.querySelector<HTMLButtonElement>(`[data-grip="${CSS.escape(d.id)}"]`)?.focus()),
                );
              }}
            >
              <div className={styles.row}>
                {/* 拖动把手：按住拖到新位置；获得焦点时 Alt+↑ / Alt+↓ 移动 */}
                <button
                  type="button"
                  className={styles.grip}
                  data-grip={d.id}
                  aria-label={tr("拖动调整顺序（Alt+↑ / Alt+↓）") + ` · ${d.name}`}
                  title={tr("拖动调整顺序（Alt+↑ / Alt+↓）")}
                  {...drag.handleProps(d.id)}
                >
                  <GripIcon />
                </button>
                <div className={styles.info}>
                  <span className={styles.name}>{d.name}</span>
                  <span className={styles.meta}>
                    {tr(KIND_LABEL[d.kind] ?? d.kind)}
                    {d.kind === "online" && ` · ${onlineMeta(d)}`}
                    {d.entryCount != null && ` · ${tr("{n} 条", { n: d.entryCount.toLocaleString() })}`}
                    {!d.loaded && <span className={styles.failed}> · {tr(d.indexState === "error" ? "索引失败" : "未能加载")}</span>}
                  </span>
                  {d.path && (
                    <span className={styles.path} title={d.path}>
                      {d.path}
                    </span>
                  )}
                </div>
                {confirming === d.id ? (
                  <span className={styles.confirm}>
                    <button type="button" className={styles.danger} onClick={() => void remove(d.id).then(() => setConfirming(null))}>
                      {tr("确认移除")}
                    </button>
                    <button type="button" className={styles.ghost} onClick={() => setConfirming(null)}>
                      {tr("取消")}
                    </button>
                  </span>
                ) : (
                  <span className={styles.confirm}>
                    {d.kind === "online" && (
                      <button
                        type="button"
                        className={styles.ghost}
                        aria-expanded={settingsOpen === d.id}
                        onClick={() => setSettingsOpen((o) => (o === d.id ? null : d.id))}
                      >
                        {tr("设置")}
                      </button>
                    )}
                    <button type="button" className={styles.ghost} onClick={() => setConfirming(d.id)} aria-label={tr("移除 {name}", { name: d.name })}>
                      {tr("移除")}
                    </button>
                  </span>
                )}
              </div>
              {settingsOpen === d.id && <OnlineSettings dict={d} />}
            </li>
          ))}
        </ul>
      )}
      <p className={styles.note}>{tr("查词结果与导出按这里的顺序排列，拖动左侧把手或用 Alt+↑ / Alt+↓ 调整。移除只删除 Lens 的索引，不会删除你的词典文件；移除在线词典会一并删除它的缓存和 Key。")}</p>
    </dialog>
  );
}
