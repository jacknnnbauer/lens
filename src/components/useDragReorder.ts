// 拖动把手调整列表顺序。用指针事件实现：Tauri 在 Windows 上会接管 HTML5 拖放（用于拖入文件），
// 原生 draggable 在 WebView 里不可靠。键盘用户另有 Alt+↑ / Alt+↓。

import { useRef, useState } from "react";

interface Drag {
  id: string;
  /** 放到这个下标之前（0…length） */
  before: number;
}

/** 把 `id` 移到原列表下标 `before` 之前，返回新顺序。 */
export function moveBefore(ids: string[], id: string, before: number): string[] {
  const from = ids.indexOf(id);
  if (from < 0) return ids;
  const next = ids.filter((x) => x !== id);
  next.splice(before > from ? before - 1 : before, 0, id);
  return next;
}

export function useDragReorder(ids: string[], onReorder: (ids: string[]) => void) {
  const [drag, setDrag] = useState<Drag | null>(null);
  const rows = useRef(new Map<string, HTMLElement>());

  const rowRef = (id: string) => (el: HTMLElement | null) => {
    if (el) rows.current.set(id, el);
    else rows.current.delete(id);
  };

  const beforeAt = (y: number): number => {
    for (let i = 0; i < ids.length; i++) {
      const r = rows.current.get(ids[i])?.getBoundingClientRect();
      if (r && y < r.top + r.height / 2) return i;
    }
    return ids.length;
  };

  const handleProps = (id: string) => ({
    onPointerDown: (e: React.PointerEvent<HTMLElement>) => {
      if (e.button !== 0) return;
      e.preventDefault();
      e.currentTarget.setPointerCapture(e.pointerId);
      setDrag({ id, before: ids.indexOf(id) });
    },
    onPointerMove: (e: React.PointerEvent<HTMLElement>) => {
      if (drag?.id !== id) return;
      const before = beforeAt(e.clientY);
      if (before !== drag.before) setDrag({ id, before });
    },
    onPointerUp: () => {
      if (drag?.id !== id) return;
      const next = moveBefore(ids, id, drag.before);
      setDrag(null);
      if (next.join("\n") !== ids.join("\n")) onReorder(next);
    },
    onPointerCancel: () => setDrag(null),
  });

  /** 行上的标记：正在拖的行、放下位置（在这一行之前 / 列表末尾） */
  const rowState = (id: string, index: number) => ({
    "data-dragging": drag?.id === id || undefined,
    "data-drop-before": (drag && drag.before === index && drag.id !== id) || undefined,
    "data-drop-after": (drag && drag.before === ids.length && index === ids.length - 1) || undefined,
  });

  return { rowRef, handleProps, rowState, dragging: drag !== null };
}
