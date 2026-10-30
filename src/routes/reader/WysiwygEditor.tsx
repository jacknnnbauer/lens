// 所见即所得编辑（阅读 › 编辑）：Milkdown（ProseMirror）在排好版的正文上直接改，改动随时转回 Markdown 交给上层。
// 输入「## 」变标题、「**粗体**」变粗体、「- 」变列表……和常见 Markdown 编辑器一样；工具栏按钮做同样的事。
// 图片和阅读视图一样不加载（换成「［图片：说明］」占位，避免联网），原 Markdown 里的图片保留不动。

import { defaultValueCtx, Editor, editorViewCtx, remarkStringifyOptionsCtx, rootCtx } from "@milkdown/kit/core";
import { clipboard } from "@milkdown/kit/plugin/clipboard";
import { history, redoCommand, undoCommand } from "@milkdown/kit/plugin/history";
import { listener, listenerCtx } from "@milkdown/kit/plugin/listener";
import {
  commonmark,
  imageSchema,
  insertHrCommand,
  toggleEmphasisCommand,
  toggleInlineCodeCommand,
  toggleStrongCommand,
  turnIntoTextCommand,
  wrapInBlockquoteCommand,
  wrapInBulletListCommand,
  wrapInHeadingCommand,
  wrapInOrderedListCommand,
} from "@milkdown/kit/preset/commonmark";
import { gfm, toggleStrikethroughCommand } from "@milkdown/kit/preset/gfm";
import type { Node as PmNode } from "@milkdown/kit/prose/model";
import { $view, callCommand, getMarkdown } from "@milkdown/kit/utils";
import { useEffect, useRef, type MutableRefObject } from "react";
import { tr } from "../../i18n";
import styles from "./ReaderPage.module.css";

// 图片节点只显示占位文字，不建 <img>
const imagePlaceholder = $view(imageSchema.node, () => (node) => {
  const dom = document.createElement("span");
  dom.className = "lens-img";
  dom.contentEditable = "false";
  const alt = String(node.attrs.alt ?? "");
  dom.textContent = alt ? tr("［图片：{alt}］", { alt }) : tr("［图片］");
  return { dom };
});

export interface WysiwygApi {
  /** 当前内容的 Markdown；和打开时相比没改过就返回 null（避免只因写法被统一就算作改动） */
  markdown: () => string | null;
}

interface Props {
  /** 打开编辑器时的 Markdown；之后的改动通过 onChange 回传，不再从这里灌回去 */
  initial: string;
  /** 改动后回传 Markdown（编辑器内部有防抖，保存时请用 api.markdown() 取最新的） */
  onChange: (md: string) => void;
  onSave: () => void;
  api: MutableRefObject<WysiwygApi | null>;
}

type Cmd = Parameters<typeof callCommand>[0];

export function WysiwygEditor({ initial, onChange, onSave, api }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const editor = useRef<Editor | null>(null);
  const cb = useRef({ onChange, onSave });
  cb.current = { onChange, onSave };

  useEffect(() => {
    const el = host.current;
    if (!el) return;
    let alive = true;
    const made = Editor.make()
      .config((ctx) => {
        ctx.set(rootCtx, el);
        ctx.set(defaultValueCtx, initial);
        // 存回 Markdown 时尽量用常见写法：列表用「-」，分隔线用「---」
        ctx.update(remarkStringifyOptionsCtx, (prev) => ({ ...prev, bullet: "-" as const, rule: "-" as const }));
        ctx.get(listenerCtx).markdownUpdated((_ctx, md, prev) => {
          if (md !== prev) cb.current.onChange(md);
        });
      })
      .use(commonmark)
      .use(gfm)
      .use(history)
      .use(listener)
      .use(clipboard)
      .use(imagePlaceholder);
    made.create().then(
      (e) => {
        if (!alive) {
          void e.destroy();
          return;
        }
        editor.current = e;
        let start: PmNode | null = null;
        e.action((ctx) => {
          const view = ctx.get(editorViewCtx);
          start = view.state.doc;
          view.focus();
        });
        api.current = {
          markdown: () => {
            let out: string | null = null;
            e.action((ctx) => {
              if (start && ctx.get(editorViewCtx).state.doc.eq(start)) return;
              out = getMarkdown()(ctx);
            });
            return out;
          },
        };
      },
      () => undefined,
    );
    return () => {
      alive = false;
      const e = editor.current;
      editor.current = null;
      api.current = null;
      void e?.destroy();
    };
    // 只在挂载时建一次：initial 之后的变化来自编辑器自己
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const run = (cmd: Cmd, payload?: unknown) => {
    const e = editor.current;
    if (!e) return;
    e.action(callCommand(cmd as string, payload));
    e.action((ctx) => ctx.get(editorViewCtx).focus());
  };

  const tools: Array<[string, string, () => void]> = [
    ["B", tr("加粗（Ctrl+B）"), () => run(toggleStrongCommand.key)],
    ["I", tr("斜体（Ctrl+I）"), () => run(toggleEmphasisCommand.key)],
    ["S", tr("删除线"), () => run(toggleStrikethroughCommand.key)],
    ["</>", tr("行内代码"), () => run(toggleInlineCodeCommand.key)],
    ["H2", tr("二级标题"), () => run(wrapInHeadingCommand.key, 2)],
    ["H3", tr("三级标题"), () => run(wrapInHeadingCommand.key, 3)],
    ["¶", tr("正文"), () => run(turnIntoTextCommand.key)],
    ["•", tr("无序列表"), () => run(wrapInBulletListCommand.key)],
    ["1.", tr("有序列表"), () => run(wrapInOrderedListCommand.key)],
    ["❝", tr("引用"), () => run(wrapInBlockquoteCommand.key)],
    ["—", tr("分隔线"), () => run(insertHrCommand.key)],
    ["↶", tr("撤销（Ctrl+Z）"), () => run(undoCommand.key)],
    ["↷", tr("重做（Ctrl+Y）"), () => run(redoCommand.key)],
  ];

  return (
    <div className={styles.wysiwyg}>
      <div role="toolbar" aria-label={tr("格式")} className={styles.fmtBar}>
        {tools.map(([label, title, act]) => (
          <button
            key={title}
            type="button"
            className={styles.fmtBtn}
            title={title}
            aria-label={title}
            // 不抢编辑器的焦点和选区
            onMouseDown={(e) => e.preventDefault()}
            onClick={act}
          >
            {label}
          </button>
        ))}
      </div>
      <div
        ref={host}
        className={`${styles.prose} ${styles.wysiwygBody}`}
        onKeyDown={(e) => {
          if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
            e.preventDefault();
            e.stopPropagation();
            cb.current.onSave();
          }
        }}
      />
    </div>
  );
}
