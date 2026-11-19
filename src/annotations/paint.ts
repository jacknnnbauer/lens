// 把批注画到正文上：按「纯文本偏移」找到对应的文本节点，切开后包进 <mark>。
// 偏移按所有文本节点顺序拼接计算，生词 span（wrapWords）只切文本节点、不改文字，所以前后一致。

export const MARK_CLASS = "lens-hl";

/** 这些元素里的纯空白文本只是块与块之间的换行 */
const BETWEEN_BLOCKS = new Set(["DIV", "SECTION", "ARTICLE", "BODY", "UL", "OL", "TABLE", "TBODY", "THEAD", "TR", "BLOCKQUOTE", "FIGURE", "NAV", "ASIDE", "HEADER", "FOOTER"]);

export interface PaintMark {
  id: number;
  start: number;
  end: number;
  color: string;
  note: boolean;
}

function textNodes(root: HTMLElement): Text[] {
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  const out: Text[] = [];
  for (let n = walker.nextNode(); n; n = walker.nextNode()) out.push(n as Text);
  return out;
}

export function plainText(root: HTMLElement): string {
  return textNodes(root)
    .map((n) => n.data)
    .join("");
}

/** 选区在纯文本里的起止；不在 root 里返回 null */
export function offsetsOf(root: HTMLElement, range: Range): [number, number] | null {
  if (!root.contains(range.startContainer) || !root.contains(range.endContainer)) return null;
  const upTo = (node: Node, offset: number) => {
    const r = document.createRange();
    r.setStart(root, 0);
    r.setEnd(node, offset);
    return r.toString().length;
  };
  const s = upTo(range.startContainer, range.startOffset);
  const e = upTo(range.endContainer, range.endOffset);
  return s < e ? [s, e] : null;
}

function wrap(root: HTMLElement, m: PaintMark) {
  let pos = 0;
  let last: HTMLElement | null = null;
  for (const node of textNodes(root)) {
    const ns = pos;
    const ne = pos + node.length;
    pos = ne;
    const a = Math.max(m.start, ns);
    const b = Math.min(m.end, ne);
    if (a >= b) continue;
    let piece = node;
    if (a > ns) piece = piece.splitText(a - ns);
    if (b < ne) piece.splitText(b - a);
    // 段落之间的换行空白不包，免得在块之间冒出一小块底色（段落里的词间空格要包，不然底色断开）
    if (!piece.data.trim() && (piece.parentElement === root || BETWEEN_BLOCKS.has(piece.parentElement?.tagName ?? ""))) continue;
    const mark = document.createElement("mark");
    mark.className = MARK_CLASS;
    mark.dataset.hl = String(m.id);
    mark.dataset.color = m.color;
    piece.replaceWith(mark);
    mark.append(piece);
    last = mark;
  }
  if (last && m.note) last.dataset.note = "";
}

/** 先加的包在外层，后加的在里层（重叠处点到的是后加的那条） */
export function paint(root: HTMLElement, marks: PaintMark[]): void {
  for (const m of [...marks].sort((x, y) => x.id - y.id)) wrap(root, m);
}

export function unpaint(root: HTMLElement): void {
  for (const m of Array.from(root.querySelectorAll<HTMLElement>(`mark.${MARK_CLASS}`))) m.replaceWith(...Array.from(m.childNodes));
  root.normalize();
}
