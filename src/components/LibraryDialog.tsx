// 词典库（M4 前的简版）：已导入词典列表、导入（文件对话框）、导入进度、删除。
// 完整的词典库页（分组、排序、配置组）见画板 03，后续里程碑实现。

import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { useEffect, useRef, useState } from "react";
import { fileName, useDicts } from "../stores/dicts";
import { CloseIcon, PlusIcon } from "./icons";
import styles from "./LibraryDialog.module.css";

const KIND_LABEL: Record<string, string> = { mdx: "MDict", stardict: "StarDict", json: "JSON", online: "在线" };

interface Props {
  open: boolean;
  onClose: () => void;
}

export function LibraryDialog({ open, onClose }: Props) {
  const dialog = useRef<HTMLDialogElement>(null);
  const { dicts, imports, error, importPaths, remove } = useDicts();
  const [confirming, setConfirming] = useState<string | null>(null);

  useEffect(() => {
    const d = dialog.current;
    if (!d) return;
    if (open && !d.open) d.showModal();
    if (!open && d.open) d.close();
  }, [open]);

  const pick = async () => {
    const picked = await openFileDialog({
      multiple: true,
      title: "选择词典文件",
      filters: [{ name: "词典（.mdx / .ifo / .json）", extensions: ["mdx", "ifo", "json"] }],
    });
    if (picked) await importPaths(Array.isArray(picked) ? picked : [picked]);
  };

  const running = Object.values(imports).filter((p) => p.phase === "start");
  const failed = Object.values(imports).filter((p) => p.phase === "error");

  return (
    <dialog ref={dialog} className={styles.dialog} onClose={onClose} aria-labelledby="library-title">
      <header className={styles.head}>
        <h2 id="library-title" className={styles.title}>
          词典库
        </h2>
        <button type="button" className={styles.primary} onClick={() => void pick()}>
          <PlusIcon />
          导入词典…
        </button>
        <button type="button" className={styles.iconBtn} aria-label="关闭" onClick={onClose}>
          <CloseIcon />
        </button>
      </header>

      {(running.length > 0 || failed.length > 0) && (
        <ul className={styles.progress} aria-live="polite">
          {running.map((p) => (
            <li key={p.path}>
              <span className={styles.spinner} aria-hidden="true" />
              正在建索引：{fileName(p.path)}（{p.index + 1}/{p.total}）
            </li>
          ))}
          {failed.map((p) => (
            <li key={p.path} className={styles.failed}>
              {fileName(p.path)}：{p.error}
            </li>
          ))}
        </ul>
      )}
      {error && <p className={styles.failed}>{error}</p>}

      {dicts.length === 0 ? (
        <p className={styles.empty}>还没有导入词典。支持 MDict（.mdx，同目录的 .mdd 自动带上）、StarDict（.ifo）和 Lens JSON。</p>
      ) : (
        <ul className={styles.list}>
          {dicts.map((d) => (
            <li key={d.id} className={styles.row}>
              <div className={styles.info}>
                <span className={styles.name}>{d.name}</span>
                <span className={styles.meta}>
                  {KIND_LABEL[d.kind] ?? d.kind}
                  {d.entryCount != null && ` · ${d.entryCount.toLocaleString()} 条`}
                  {!d.loaded && <span className={styles.failed}> · {d.indexState === "error" ? "索引失败" : "未能加载"}</span>}
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
                    确认移除
                  </button>
                  <button type="button" className={styles.ghost} onClick={() => setConfirming(null)}>
                    取消
                  </button>
                </span>
              ) : (
                <button type="button" className={styles.ghost} onClick={() => setConfirming(d.id)} aria-label={`移除 ${d.name}`}>
                  移除
                </button>
              )}
            </li>
          ))}
        </ul>
      )}
      <p className={styles.note}>移除只删除 Lens 的索引，不会删除你的词典文件。</p>
    </dialog>
  );
}
