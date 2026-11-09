// 精读本的「图解」：AI 从九种图示里挑出适合这本书的几种并填好数据（后端 digest.rs 的 Diagrams），这里排成静态 SVG 印进 PDF。
// 不靠脚本也不靠浏览器测量：文字宽度按字号估算（中文一个字约一个字号宽），布局全部在这里算好。
// 颜色用 export.css 里的 --dg-* 变量，跟着导出样式走。

import { escapeHtml } from "../copy/convert";
import { tr } from "../i18n";
import type { DiagramGraph, Diagrams, DigestChapter, DiagramLink, DiagramTree } from "../ipc/digest";

/** 估算文字宽度：中日韩字符约一个字号，其他约 0.56 个字号 */
export function textWidth(s: string, size: number): number {
  let w = 0;
  for (const c of s) w += /[⺀-￿]/.test(c) ? size : size * 0.56;
  return w;
}

/** 按宽度折行 */
export function wrap(s: string, size: number, width: number): string[] {
  const lines: string[] = [];
  let cur = "";
  for (const c of s) {
    if (cur && textWidth(cur + c, size) > width) {
      lines.push(cur);
      cur = "";
    }
    cur += c;
  }
  if (cur) lines.push(cur);
  return lines;
}

const e = escapeHtml;
const n = (x: number) => Math.round(x * 10) / 10;

function text(x: number, y: number, s: string, size: number, opts: { anchor?: "start" | "middle" | "end"; weight?: number; color?: string; halo?: boolean } = {}): string {
  const style = [`font-size:${size}px`, opts.weight ? `font-weight:${opts.weight}` : "", opts.color ? `fill:var(${opts.color})` : ""].filter(Boolean).join(";");
  return `<text x="${n(x)}" y="${n(y)}"${opts.anchor && opts.anchor !== "start" ? ` text-anchor="${opts.anchor}"` : ""}${opts.halo ? ' class="dg-halo"' : ""} style="${style}">${e(s)}</text>`;
}

function svg(w: number, h: number, body: string, label: string): string {
  return `<svg class="dg-svg" viewBox="0 0 ${n(w)} ${n(h)}" role="img" aria-label="${e(label)}" xmlns="http://www.w3.org/2000/svg">${body}</svg>`;
}

const KIND_COLOR: Record<string, string> = {
  conflict: "--dg-conflict",
  bond: "--dg-bond",
  rank: "--dg-rank",
  fate: "--dg-fate",
  cause: "--dg-cause",
  bad: "--dg-conflict",
  warn: "--dg-fate",
  good: "--dg-bond",
  neutral: "--dg-rank",
};
const color = (k: string) => KIND_COLOR[k] ?? "--dg-rank";
/** 分组的浅底色，按分组先后轮着用 */
const GROUP_FILLS = ["--dg-g1", "--dg-g2", "--dg-g3", "--dg-g4", "--dg-g5", "--dg-g6", "--dg-g7", "--dg-g8"];

function legend(items: Array<[string, string, boolean?]>): string {
  return `<p class="dg-legend">${items.map(([c, label, dash]) => `<span><i style="border-top:2.5px ${dash ? "dashed" : "solid"} var(${c})"></i>${e(label)}</span>`).join("")}</p>`;
}

// —— 人物关系图：主角居中，内圈是和他直接相关的人，外圈挂在关系最近的内圈人物旁边 ——

export function relationsSvg(d: Diagrams["relations"]): string {
  const listed = d.people.filter((p) => p.id);
  const ids = new Set(listed.map((p) => p.id));
  const links = d.links.filter((l) => ids.has(l.a) && ids.has(l.b) && l.a !== l.b);
  const linked = new Set(links.flatMap((l) => [l.a, l.b]));
  const center = ids.has(d.center) ? d.center : (listed.find((p) => linked.has(p.id))?.id ?? "");
  // 没有任何关系的人不画（画出来只是一个孤零零的框）
  const people = listed.filter((p) => p.id === center || linked.has(p.id));
  if (people.length < 2) return "";
  const groupIds = [...d.groups.map((g) => g.id), ...people.map((p) => p.group)].filter((g, i, a) => g && a.indexOf(g) === i);
  const fill = (g: string) => GROUP_FILLS[Math.max(0, groupIds.indexOf(g)) % GROUP_FILLS.length];
  const byId = new Map(people.map((p) => [p.id, p]));
  const neighbors = (id: string) => links.filter((l) => l.a === id || l.b === id).map((l) => (l.a === id ? l.b : l.a));
  const rel = (x: string, y: string) => links.find((l) => (l.a === x && l.b === y) || (l.a === y && l.b === x));
  const PRIORITY: Record<string, number> = { bond: 0, conflict: 1, fate: 2, rank: 3 };

  const inner = neighbors(center).filter((v, i, a) => a.indexOf(v) === i);
  inner.sort((a, b) => groupIds.indexOf(byId.get(a)?.group ?? "") - groupIds.indexOf(byId.get(b)?.group ?? ""));
  // 外圈：按离主角的远近一层层找，每人挂在把他引进来的那个内圈人物下面（关系近的优先：友谊 > 冲突 > 预言 > 上下级）
  const parent = new Map<string, string>();
  let frontier = [...inner];
  const anchor = new Map(inner.map((id) => [id, id]));
  while (frontier.length) {
    const next: string[] = [];
    for (const id of frontier) {
      const ns = neighbors(id).filter((x) => x !== center && !inner.includes(x) && !parent.has(x));
      ns.sort((x, y) => (PRIORITY[rel(id, x)?.kind ?? ""] ?? 9) - (PRIORITY[rel(id, y)?.kind ?? ""] ?? 9));
      for (const x of ns) {
        parent.set(x, anchor.get(id)!);
        anchor.set(x, anchor.get(id)!);
        next.push(x);
      }
    }
    frontier = next;
  }
  // 和主角那一片连不上的人：单独一个扇区（不画扇区的中心）
  const OTHERS = "\u0000others";
  const loose = people.filter((p) => p.id !== center && !inner.includes(p.id) && !parent.has(p.id));
  for (const p of loose) parent.set(p.id, OTHERS);
  if (loose.length) inner.push(OTHERS);
  const kids = (id: string) => [...parent.entries()].filter(([, v]) => v === id).map(([k]) => k);
  const weight = (id: string) => 1 + kids(id).length * 0.9;
  const total = inner.reduce((s, id) => s + weight(id), 0);
  // 内圈人多时半径放大，框不挤在一起
  const R1 = Math.max(205, (inner.length * 100) / (2 * Math.PI));
  const R2 = parent.size ? R1 + 140 : 0;
  const cx = 0;
  const cy = 0;
  const pos = new Map<string, { x: number; y: number }>([[center, { x: cx, y: cy }]]);
  // 挂人最多的放在正下方，那里最宽
  const heavy = inner.filter((id) => id !== OTHERS).reduce((m, id) => (weight(id) > weight(m) ? id : m), inner[0]);
  let ang = Math.PI / 2;
  for (const id of inner) {
    if (id === heavy) break;
    ang -= (weight(id) / total) * 2 * Math.PI;
  }
  ang -= (weight(heavy) / total) * Math.PI;
  for (const id of inner) {
    const span = (weight(id) / total) * 2 * Math.PI;
    const a = ang + span / 2;
    if (id !== OTHERS) pos.set(id, { x: cx + R1 * Math.cos(a), y: cy + R1 * Math.sin(a) });
    const ks = kids(id);
    ks.forEach((k, i) => {
      const b = ang + (span * (i + 1)) / (ks.length + 1);
      pos.set(k, { x: cx + R2 * Math.cos(b), y: cy + R2 * Math.sin(b) });
    });
    ang += span;
  }

  // 力导向微调：有关系的互相拉近，所有人互相推开（框重叠时推得更狠），主角钉在中间
  const size = new Map(
    people.map((p) => {
      const main = p.id === center;
      const fs = main ? 18 : 14;
      return [p.id, { w: Math.max(textWidth(p.name, fs), textWidth(p.en, 10.5)) + 22, h: main ? 50 : p.en ? 40 : 30 }];
    }),
  );
  relax(pos, size, links, center);
  // 按实际占用的范围平移、缩放到画布
  const xsAll = [...pos].map(([id, q]) => [q.x - size.get(id)!.w / 2, q.x + size.get(id)!.w / 2]).flat();
  const ysAll = [...pos].map(([id, q]) => [q.y - size.get(id)!.h / 2, q.y + size.get(id)!.h / 2]).flat();
  const minX = Math.min(...xsAll);
  const minY = Math.min(...ysAll);
  const spanX = Math.max(...xsAll) - minX;
  const spanY = Math.max(...ysAll) - minY;
  const W = 820;
  const scale = Math.min(1, (W - 20) / spanX);
  const H = spanY * scale + 60;
  for (const q of pos.values()) {
    q.x = 10 + (q.x - minX) * scale + (W - 20 - spanX * scale) / 2;
    q.y = 44 + (q.y - minY) * scale;
  }

  let lines = "";
  let labels = "";
  for (const l of links) {
    const p = pos.get(l.a);
    const q = pos.get(l.b);
    if (!p || !q) continue;
    const fromCenter = l.a === center || l.b === center;
    const dx = q.x - p.x;
    const dy = q.y - p.y;
    const len = Math.hypot(dx, dy) || 1;
    const bend = fromCenter ? 0 : 22;
    const qx = (p.x + q.x) / 2 - (dy / len) * bend;
    const qy = (p.y + q.y) / 2 + (dx / len) * bend;
    const c = color(l.kind);
    lines += `<path d="M${n(p.x)},${n(p.y)} Q${n(qx)},${n(qy)} ${n(q.x)},${n(q.y)}" fill="none" style="stroke:var(${c})" stroke-width="${l.kind === "rank" ? 1.4 : 2.2}"${l.kind === "fate" ? ' stroke-dasharray="6 4"' : ""}/>`;
    const t = fromCenter ? (l.a === center ? 0.6 : 0.4) : 0.5;
    const lx = (1 - t) ** 2 * p.x + 2 * (1 - t) * t * qx + t * t * q.x;
    const ly = (1 - t) ** 2 * p.y + 2 * (1 - t) * t * qy + t * t * q.y;
    if (l.label) labels += text(lx, ly + 4, l.label, 11.5, { anchor: "middle", color: c, halo: true });
  }
  let nodes = "";
  for (const p of people) {
    const at = pos.get(p.id);
    if (!at) continue;
    const main = p.id === center;
    const fs = main ? 18 : 14;
    const w = Math.max(textWidth(p.name, fs), textWidth(p.en, 10.5)) + 22;
    const h = main ? 50 : p.en ? 40 : 30;
    nodes += `<rect x="${n(at.x - w / 2)}" y="${n(at.y - h / 2)}" width="${n(w)}" height="${h}" rx="10" style="fill:var(${fill(p.group)});stroke:var(${main ? "--dg-conflict" : "--dg-line"})" stroke-width="${main ? 2 : 1}"/>`;
    nodes += text(at.x, at.y + (p.en ? (main ? -3 : -2) : 5), p.name, fs, { anchor: "middle", weight: 700 });
    if (p.en) nodes += text(at.x, at.y + (main ? 15 : 13), p.en, 10.5, { anchor: "middle", color: "--dg-sub" });
  }
  // 分组图例
  let gx = 10;
  let groups = "";
  for (const g of groupIds) {
    const label = d.groups.find((x) => x.id === g)?.label || g;
    groups += `<rect x="${gx}" y="10" width="14" height="14" rx="3" style="fill:var(${fill(g)});stroke:var(--dg-line)"/>${text(gx + 19, 22, label, 12, { color: "--dg-sub" })}`;
    gx += 33 + textWidth(label, 12);
  }
  return (
    legend([
      ["--dg-conflict", tr("对立、冲突")],
      ["--dg-bond", tr("友谊、亲情、救助")],
      ["--dg-rank", tr("上下级、雇佣")],
      ["--dg-fate", tr("预言、警告、宿命"), true],
    ]) + svg(W, H, groups + lines + labels + nodes, tr("人物关系图"))
  );
}

/** 力导向布局（Fruchterman–Reingold）：从圆环布局出发迭代，确定性的（没有随机数） */
function relax(pos: Map<string, { x: number; y: number }>, size: Map<string, { w: number; h: number }>, links: DiagramLink[], pinned: string): void {
  const ids = [...pos.keys()];
  const K = 175;
  const ITER = 350;
  for (let it = 0; it < ITER; it++) {
    const temp = 40 * (1 - it / ITER) + 1;
    const disp = new Map(ids.map((id) => [id, { x: 0, y: 0 }]));
    for (let i = 0; i < ids.length; i++)
      for (let j = i + 1; j < ids.length; j++) {
        const a = pos.get(ids[i])!;
        const b = pos.get(ids[j])!;
        let dx = a.x - b.x;
        let dy = a.y - b.y;
        if (dx === 0 && dy === 0) dx = 0.1 * (i - j);
        const d = Math.hypot(dx, dy);
        const sa = size.get(ids[i])!;
        const sb = size.get(ids[j])!;
        const overlap = Math.abs(dx) < (sa.w + sb.w) / 2 + 26 && Math.abs(dy) < (sa.h + sb.h) / 2 + 22;
        const f = ((K * K) / d) * (overlap ? 4 : 1);
        dx /= d;
        dy /= d;
        const da = disp.get(ids[i])!;
        const db = disp.get(ids[j])!;
        da.x += dx * f;
        da.y += dy * f;
        db.x -= dx * f;
        db.y -= dy * f;
      }
    for (const l of links) {
      const a = pos.get(l.a);
      const b = pos.get(l.b);
      if (!a || !b) continue;
      const dx = a.x - b.x;
      const dy = a.y - b.y;
      const d = Math.hypot(dx, dy) || 1;
      const f = (d * d) / K / 2;
      const da = disp.get(l.a)!;
      const db = disp.get(l.b)!;
      da.x -= (dx / d) * f;
      da.y -= (dy / d) * f;
      db.x += (dx / d) * f;
      db.y += (dy / d) * f;
    }
    const c = pos.get(pinned);
    for (const id of ids) {
      if (id === pinned) continue;
      const q = pos.get(id)!;
      const v = disp.get(id)!;
      // 往主角方向的一点引力，免得小团体漂走
      if (c) {
        v.x -= (q.x - c.x) * 0.03;
        v.y -= (q.y - c.y) * 0.03;
      }
      const len = Math.hypot(v.x, v.y) || 1;
      const step = Math.min(len, temp);
      q.x += (v.x / len) * step;
      // 纵向压一点：页面比画布宽高比更「矮胖」时放得下
      q.y += (v.y / len) * step * 0.85;
    }
  }
}

// —— 故事进展与伏笔图：竖直主线按全书几大部分分段，右侧弧线把伏笔连到应验 ——

export interface PartBand {
  title: string;
  from: number;
  to: number;
}

export function plotSvg(d: Diagrams["plot"], parts: PartBand[], label: (idx: number) => string): string {
  const events = d.events.filter((x) => x.id && x.text).slice(0, 32);
  if (events.length < 3) return "";
  const W = 800;
  const SPINE = 150;
  const TEXT_X = 166;
  const CH_W = 70;
  const ARC_X = 520;
  const FS = 13.5;
  const LINE_H = 19;
  const PAD = 12;
  const partOf = (ch: number) => parts.findIndex((p) => ch >= p.from && ch <= p.to);
  let y = 14;
  const rows = events.map((ev) => {
    const lines = wrap(ev.text, FS, ARC_X - TEXT_X - CH_W - 16);
    const h = lines.length * LINE_H + PAD;
    const row = { ...ev, lines, y, cy: y + LINE_H / 2 + 2, h, part: partOf(ev.chapter) };
    y += h;
    return row;
  });
  let H = y + 12;
  const byId = new Map(rows.map((r) => [r.id, r]));
  let body = "";
  // 各部分的色带
  let band = 0;
  for (let i = 0; i < rows.length; ) {
    let j = i;
    while (j + 1 < rows.length && rows[j + 1].part === rows[i].part) j++;
    const y0 = rows[i].y - 4;
    const y1 = rows[j].y + rows[j].h - 4;
    if (band % 2 === 0) body += `<rect x="0" y="${n(y0)}" width="${W}" height="${n(y1 - y0)}" rx="6" style="fill:var(--dg-band)"/>`;
    const title = rows[i].part >= 0 ? parts[rows[i].part].title : "";
    wrap(title, 13, SPINE - 26)
      .slice(0, 3)
      .forEach((ln, k) => (body += text(10, y0 + 20 + k * 17, ln, 13, { weight: 700 })));
    band++;
    i = j + 1;
  }
  body += `<line x1="${SPINE}" y1="${n(rows[0].cy)}" x2="${SPINE}" y2="${n(rows[rows.length - 1].cy)}" style="stroke:var(--dg-text)" stroke-width="2.5"/>`;
  const links = d.links.filter((l) => byId.has(l.a) && byId.has(l.b) && l.a !== l.b);
  const linked = new Set(links.flatMap((l) => [l.a, l.b]));
  const last = parts.length - 1;
  for (const r of rows) {
    const climax = r.part === last && last > 0;
    body += `<circle cx="${SPINE}" cy="${n(r.cy)}" r="${climax ? 7 : 5.5}" style="fill:var(${climax ? "--dg-conflict" : "--dg-paper"});stroke:var(--dg-text)" stroke-width="2"/>`;
    body += text(TEXT_X, r.cy + 4.5, label(r.chapter), 11, { color: "--dg-sub" });
    r.lines.forEach((ln, i) => (body += text(TEXT_X + CH_W, r.cy + 4.5 + i * LINE_H, ln, FS)));
    if (linked.has(r.id)) {
      const end = TEXT_X + CH_W + Math.max(...r.lines.map((l) => textWidth(l, FS))) + 6;
      body += `<line x1="${n(end)}" y1="${n(r.cy)}" x2="${ARC_X}" y2="${n(r.cy)}" style="stroke:var(--dg-line)" stroke-dasharray="2 3"/><circle cx="${ARC_X}" cy="${n(r.cy)}" r="3" style="fill:var(--dg-text)"/>`;
    }
  }
  // 弧线：区间重叠的放到更外层
  type Arc = DiagramLink & { y1: number; y2: number; top: number; bot: number; level: number; no: number };
  const arcs: Arc[] = links.map((l) => {
    const y1 = byId.get(l.a)!.cy;
    const y2 = byId.get(l.b)!.cy;
    return { ...l, y1, y2, top: Math.min(y1, y2), bot: Math.max(y1, y2), level: 0, no: 0 };
  });
  arcs.sort((a, b) => a.bot - a.top - (b.bot - b.top));
  const placed: Arc[] = [];
  for (const a of arcs) {
    let level = 0;
    while (placed.some((p) => p.level === level && !(p.bot < a.top - 6 || p.top > a.bot + 6))) level++;
    a.level = level;
    placed.push(a);
  }
  const maxLevel = Math.max(0, ...placed.map((p) => p.level));
  const step = Math.min(30, (W - ARC_X - 50) / (maxLevel + 1));
  const numbered = [...placed].sort((a, b) => a.top - b.top || a.bot - b.bot);
  numbered.forEach((a, i) => (a.no = i + 1));
  for (const a of placed) {
    const w = 18 + a.level * step;
    const c = a.kind === "fate" ? "--dg-fate" : "--dg-cause";
    body += `<path d="M${ARC_X},${n(a.y1)} C${n(ARC_X + w * 1.3)},${n(a.y1)} ${n(ARC_X + w * 1.3)},${n(a.y2)} ${ARC_X},${n(a.y2)}" fill="none" style="stroke:var(${c})" stroke-width="1.8"${a.kind === "fate" ? ' stroke-dasharray="5 4"' : ""}/>`;
    // 箭头：画在终点，朝左
    body += `<path d="M${ARC_X + 1},${n(a.y2)} l8,-4.5 l0,9 z" style="fill:var(${c})"/>`;
    const bx = ARC_X + w * 0.975;
    const by = (a.y1 + a.y2) / 2;
    body += `<circle cx="${n(bx)}" cy="${n(by)}" r="8.5" style="fill:var(--dg-paper);stroke:var(${c})" stroke-width="1.5"/>`;
    body += text(bx, by + 4, String(a.no), 10.5, { anchor: "middle", weight: 700, color: c });
  }
  // 图下面的说明，两列
  const colW = (W - 20) / 2;
  numbered.forEach((a, i) => {
    const x = 10 + (i % 2) * colW;
    const yy = H + Math.floor(i / 2) * 24 + 14;
    const c = a.kind === "fate" ? "--dg-fate" : "--dg-cause";
    body += `<circle cx="${x + 9}" cy="${n(yy - 4)}" r="8.5" style="fill:var(--dg-paper);stroke:var(${c})" stroke-width="1.5"/>`;
    body += text(x + 9, yy, String(a.no), 10.5, { anchor: "middle", weight: 700, color: c });
    body += text(x + 24, yy, `${a.label}：${label(byId.get(a.a)!.chapter)} → ${label(byId.get(a.b)!.chapter)}`, 12.5);
  });
  H += Math.ceil(numbered.length / 2) * 24 + 10;
  return (
    legend([
      ["--dg-fate", tr("伏笔 → 应验"), true],
      ["--dg-cause", tr("前因 → 后果")],
    ]) + svg(W, H, body, tr("故事进展与伏笔"))
  );
}

// —— 组织 / 等级树、家谱：自上而下，叶子平均分宽度，上级在下级中间 ——

interface TNode {
  id: string;
  name: string;
  role: string;
  special: boolean;
  children: TNode[];
  depth: number;
  x: number;
  span: number;
}

export function treeSvg(d: DiagramTree, label: string): string {
  const raw = d.nodes.filter((x) => x.id && x.name);
  if (raw.length < 2) return "";
  const map = new Map<string, TNode>(raw.map((x) => [x.id, { id: x.id, name: x.name, role: x.role, special: x.special, children: [], depth: 0, x: 0, span: 1 }]));
  const roots: TNode[] = [];
  for (const x of raw) {
    const node = map.get(x.id)!;
    const p = x.parent && x.parent !== x.id ? map.get(x.parent) : undefined;
    if (p) p.children.push(node);
    else roots.push(node);
  }
  // 有环时（AI 写错）没有根：随便取第一个
  if (!roots.length) roots.push(map.get(raw[0].id)!);
  const W = 800;
  const LEVEL = 78;
  const NODE_H = 42;
  const leaves: TNode[] = [];
  const seen = new Set<string>();
  const walk = (t: TNode, depth: number) => {
    if (seen.has(t.id)) return;
    seen.add(t.id);
    t.depth = depth;
    t.children = t.children.filter((c) => !seen.has(c.id));
    t.children.forEach((c) => walk(c, depth + 1));
    if (!t.children.length) leaves.push(t);
  };
  roots.forEach((r) => walk(r, 0));
  const slot = (W - 20) / leaves.length;
  leaves.forEach((t, i) => (t.x = 10 + slot * (i + 0.5)));
  const place = (t: TNode): number => {
    if (t.children.length) {
      t.span = t.children.map(place).reduce((a, b) => a + b, 0);
      t.x = (t.children[0].x + t.children[t.children.length - 1].x) / 2;
    }
    return t.span;
  };
  roots.forEach(place);
  const all = [...seen].map((id) => map.get(id)!);
  const depth = Math.max(...all.map((t) => t.depth));
  const H = 20 + depth * LEVEL + NODE_H + 10;
  const y = (t: TNode) => 10 + t.depth * LEVEL;
  let body = "";
  for (const t of all)
    for (const c of t.children) {
      const y1 = y(t) + NODE_H;
      const y2 = y(c);
      const my = (y1 + y2) / 2;
      body += `<path d="M${n(t.x)},${n(y1)} V${n(my)} H${n(c.x)} V${n(y2)}" fill="none" style="stroke:var(${c.special ? "--dg-fate" : "--dg-rank"})" stroke-width="1.5"${c.special ? ' stroke-dasharray="5 4"' : ""}/>`;
    }
  const fsName = leaves.length > 9 ? 12.5 : 14;
  const fsRole = leaves.length > 9 ? 10 : 11;
  for (const t of all) {
    const roleLines = wrap(t.role, fsRole, Math.max(60, slot * t.span - 18)).slice(0, 1);
    const w = Math.min(slot * t.span - 6, Math.max(textWidth(t.name, fsName), ...roleLines.map((l) => textWidth(l, fsRole))) + 18);
    const top = t.depth === 0;
    body += `<rect x="${n(t.x - w / 2)}" y="${n(y(t))}" width="${n(w)}" height="${NODE_H}" rx="9" style="fill:var(${top ? "--dg-g1" : t.special ? "--dg-g5" : "--dg-paper"});stroke:var(${t.special ? "--dg-fate" : "--dg-line"})" stroke-width="1"${t.special ? ' stroke-dasharray="5 4"' : ""}/>`;
    body += text(t.x, y(t) + (roleLines.length ? 18 : 26), t.name, fsName, { anchor: "middle", weight: 700 });
    if (roleLines.length) body += text(t.x, y(t) + 34, roleLines[0], fsRole, { anchor: "middle", color: "--dg-sub" });
  }
  return svg(W, H, body, label);
}

// —— 地点路线图（示意）：一条起伏的航迹串起各站，站名和事件上下交替 ——

export function routeSvg(d: Diagrams["route"], label: (idx: number) => string): string {
  const stops = d.stops.filter((s) => s.place).slice(0, 10);
  if (stops.length < 2) return "";
  const W = 800;
  const H = 330;
  const k = stops.length;
  const xs = stops.map((_, i) => 90 + (i * (W - 180)) / Math.max(1, k - 1));
  const ys = stops.map((_, i) => 160 + (i % 2 ? 40 : -20));
  let path = `M${n(xs[0])},${n(ys[0])}`;
  for (let i = 1; i < k; i++) {
    const mx = (xs[i - 1] + xs[i]) / 2;
    path += ` C${n(mx)},${n(ys[i - 1])} ${n(mx)},${n(ys[i])} ${n(xs[i])},${n(ys[i])}`;
  }
  let body = `<rect x="0" y="40" width="${W}" height="240" rx="14" style="fill:var(--dg-sea)"/>`;
  for (let j = 0; j < 4; j++) body += `<path d="M20,${80 + j * 55} q20,-8 40,0 t40,0 t40,0" fill="none" style="stroke:var(--dg-line)" stroke-width="1.2"/>`;
  body += `<path d="${path}" fill="none" style="stroke:var(--dg-cause)" stroke-width="3" stroke-dasharray="10 6"/>`;
  const slotW = (W - 180) / Math.max(1, k - 1) + 30;
  stops.forEach((s, i) => {
    const end = i === k - 1;
    body += `<circle cx="${n(xs[i])}" cy="${n(ys[i])}" r="${end ? 9 : 7}" style="fill:var(${end ? "--dg-conflict" : "--dg-paper"});stroke:var(${end ? "--dg-conflict" : "--dg-cause"})" stroke-width="2.5"/>`;
    const above = i % 2 === 0;
    const what = wrap(s.what, 12, slotW * 1.6).slice(0, 2);
    const ty = above ? ys[i] - 50 - what.length * 16 : ys[i] + 32;
    body += text(xs[i], ty, s.place, 14, { anchor: "middle", weight: 700 });
    const ch = s.chapter ? label(s.chapter) : "";
    if (ch) body += text(xs[i], ty + 17, ch, 11, { anchor: "middle", color: "--dg-sub" });
    what.forEach((ln, j) => (body += text(xs[i], ty + (ch ? 34 : 18) + j * 16, ln, 12, { anchor: "middle" })));
  });
  body += text(W - 12, H - 8, tr("示意图，不按真实比例"), 11, { anchor: "end", color: "--dg-sub" });
  return svg(W, H, body, tr("地点路线"));
}

// —— 全书节奏：每章一格，颜色是情节 / 题外话，高度是紧张程度，红线是前后各两章的平均 ——

export function rhythmSvg(chapters: DigestChapter[], d: Diagrams["rhythm"], parts: PartBand[], short: (idx: number) => string): string {
  const chs = chapters.filter((c) => c.kind !== "other" || c.tension > 0);
  if (chs.length < 5) return "";
  const W = 800;
  const H = 270;
  const base = 200;
  const top = 46;
  const x0 = 20;
  const bw = (W - 2 * x0) / chs.length;
  const level = (c: DigestChapter) => Math.min(5, Math.max(1, c.tension || (c.kind === "digression" ? 1 : 2)));
  let body = "";
  for (let v = 1; v <= 5; v++) {
    const yy = base - ((base - top) * v) / 5;
    body += `<line x1="${x0}" x2="${W - x0}" y1="${n(yy)}" y2="${n(yy)}" style="stroke:var(--dg-line)" stroke-width="1"/>`;
  }
  chs.forEach((c, i) => {
    const h = ((base - top) * level(c)) / 5;
    const dig = c.kind === "digression";
    body += `<rect x="${n(x0 + i * bw + 0.3)}" y="${n(base - h)}" width="${n(Math.max(0.8, bw - 0.6))}" height="${n(h)}" style="fill:var(${dig ? "--dg-rank" : "--dg-cause"})" opacity="${dig ? 0.45 : 0.85}"/>`;
  });
  let line = "";
  chs.forEach((_, i) => {
    const win = chs.slice(Math.max(0, i - 2), i + 3).map(level);
    const v = win.reduce((a, b) => a + b, 0) / win.length;
    line += `${i ? " L" : "M"}${n(x0 + (i + 0.5) * bw)},${n(base - ((base - top) * v) / 5)}`;
  });
  body += `<path d="${line}" fill="none" style="stroke:var(--dg-conflict)" stroke-width="2"/>`;
  // 高潮点：标签太近时错开一行
  let lastX = -1e9;
  let row = 0;
  for (const m of d.marks) {
    const i = chs.findIndex((c) => c.idx === m.chapter);
    if (i < 0) continue;
    const px = x0 + (i + 0.5) * bw;
    row = px - lastX < textWidth(m.label, 11.5) + 10 ? 1 - row : 0;
    lastX = px;
    const ly = top - 14 - row * 15;
    body += `<line x1="${n(px)}" x2="${n(px)}" y1="${n(ly + 4)}" y2="${base}" style="stroke:var(--dg-conflict)" stroke-width="1" stroke-dasharray="2 3"/>`;
    body += text(px, ly, m.label, 11.5, { anchor: "middle", color: "--dg-conflict", halo: true });
  }
  // 各部分
  parts.forEach((p) => {
    const a = chs.findIndex((c) => c.idx >= p.from);
    let b = chs.findIndex((c) => c.idx > p.to);
    if (a < 0) return;
    if (b < 0) b = chs.length;
    const xa = x0 + a * bw;
    const xb = x0 + b * bw;
    body += `<line x1="${n(xa)}" x2="${n(xa)}" y1="${base}" y2="${base + 30}" style="stroke:var(--dg-text)" stroke-width="1"/>`;
    const t = wrap(p.title, 11.5, Math.max(30, xb - xa - 6))[0] ?? "";
    body += text((xa + xb) / 2, base + 20, t + (t.length < p.title.length ? "…" : ""), 11.5, { anchor: "middle" });
  });
  body += text(x0, H - 6, tr("{a} – {b}；红线是前后各两章的平均", { a: short(chs[0].idx), b: short(chs[chs.length - 1].idx) }), 11, { color: "--dg-sub" });
  return (
    legend([
      ["--dg-cause", tr("情节章")],
      ["--dg-rank", tr("题外话章")],
      ["--dg-conflict", tr("紧张程度走势")],
    ]) + svg(W, H, body, tr("全书节奏"))
  );
}

// —— 对照图：每项一张卡片，三列 ——

export function compareSvg(d: Diagrams["compare"], label: (idx: number) => string): string {
  const items = d.items.filter((x) => x.name).slice(0, 12);
  if (items.length < 2) return "";
  const W = 800;
  const cols = items.length <= 4 ? 2 : 3;
  const gap = 12;
  const cw = (W - gap * (cols - 1)) / cols;
  const lines = items.map((x) => wrap(x.text, 12.5, cw - 32).slice(0, 4));
  const ch = 82 + Math.max(...lines.map((l) => l.length)) * 18;
  let body = "";
  items.forEach((it, i) => {
    const x = (i % cols) * (cw + gap);
    const y = Math.floor(i / cols) * (ch + gap);
    const c = color(it.tone);
    body += `<rect x="${n(x)}" y="${n(y)}" width="${n(cw)}" height="${ch}" rx="10" style="fill:var(--dg-paper);stroke:var(--dg-line)"/>`;
    body += `<rect x="${n(x)}" y="${n(y)}" width="6" height="${ch}" rx="3" style="fill:var(${c})"/>`;
    const meta = [tr("第 {n} 项", { n: i + 1 }), it.chapter ? label(it.chapter) : ""].filter(Boolean).join(" · ");
    body += text(x + 18, y + 24, meta, 11, { color: "--dg-sub" });
    body += text(x + 18, y + 46, it.name, 15, { weight: 700 });
    if (it.en) body += text(x + 18, y + 62, it.en, 11, { color: "--dg-sub" });
    if (it.badge) {
      const bw = textWidth(it.badge, 11) + 14;
      body += `<rect x="${n(x + cw - bw - 12)}" y="${n(y + 12)}" width="${n(bw)}" height="20" rx="10" style="fill:var(${c})" opacity="0.15"/>`;
      body += text(x + cw - bw / 2 - 12, y + 26, it.badge, 11, { anchor: "middle", weight: 700, color: c });
    }
    lines[i].forEach((ln, k) => (body += text(x + 18, y + 86 + k * 18, ln, 12.5)));
  });
  const rows = Math.ceil(items.length / cols);
  return (
    legend([
      ["--dg-conflict", tr("凶兆、伤亡")],
      ["--dg-fate", tr("警告")],
      ["--dg-bond", tr("喜剧、反衬")],
    ]) + svg(W, rows * (ch + gap) - gap, body, d.title || tr("对照"))
  );
}

// —— 因果链 / 论证结构：自上而下分层（按最长路径），概念关系图：排成一圈 ——

const NODE_KIND: Record<string, string> = { claim: "--dg-g1", reason: "--dg-g2", evidence: "--dg-g3", cause: "--dg-g4", effect: "--dg-g1", concept: "--dg-g2" };

export function graphSvg(d: DiagramGraph, mode: "layers" | "circle", label: string): string {
  const nodes = d.nodes.filter((x) => x.id && x.text).slice(0, 14);
  if (nodes.length < 2) return "";
  const ids = new Set(nodes.map((x) => x.id));
  const links = d.links.filter((l) => ids.has(l.a) && ids.has(l.b) && l.a !== l.b);
  const W = 800;
  const BOX_W = 150;
  const FS = 12.5;
  const boxLines = new Map(nodes.map((x) => [x.id, wrap(x.text, FS, BOX_W - 16).slice(0, 3)]));
  const boxH = (id: string) => (boxLines.get(id)?.length ?? 1) * 17 + 16;
  const pos = new Map<string, { x: number; y: number }>();
  let H: number;
  if (mode === "layers") {
    // 层号 = 从没有入边的点出发的最长路径（有环时最多走节点数那么多步）
    const layer = new Map(nodes.map((x) => [x.id, 0]));
    for (let k = 0; k < nodes.length; k++)
      for (const l of links) if ((layer.get(l.b) ?? 0) < (layer.get(l.a) ?? 0) + 1 && (layer.get(l.a) ?? 0) + 1 < nodes.length) layer.set(l.b, (layer.get(l.a) ?? 0) + 1);
    const maxL = Math.max(...layer.values());
    const LEVEL = 110;
    H = (maxL + 1) * LEVEL + 10;
    for (let L = 0; L <= maxL; L++) {
      const row = nodes.filter((x) => layer.get(x.id) === L);
      row.forEach((x, i) => pos.set(x.id, { x: ((i + 0.5) * W) / row.length, y: 10 + L * LEVEL + boxH(x.id) / 2 }));
    }
  } else {
    const R = Math.max(150, Math.min(260, nodes.length * 26));
    H = 2 * R + 110;
    nodes.forEach((x, i) => {
      const a = -Math.PI / 2 + (i * 2 * Math.PI) / nodes.length;
      pos.set(x.id, { x: W / 2 + R * 1.25 * Math.cos(a), y: H / 2 + R * Math.sin(a) });
    });
  }
  let lines = "";
  let labels = "";
  for (const l of links) {
    const p = pos.get(l.a)!;
    const q = pos.get(l.b)!;
    // 线从框边出发：上下排列时从框底到框顶，绕圈时到框的中心再由框遮住
    const y1 = mode === "layers" ? p.y + boxH(l.a) / 2 : p.y;
    const y2 = mode === "layers" ? q.y - boxH(l.b) / 2 - 4 : q.y;
    lines += `<path d="M${n(p.x)},${n(y1)} L${n(q.x)},${n(y2)}" fill="none" style="stroke:var(--dg-cause)" stroke-width="1.6"/>`;
    if (mode === "layers") lines += `<path d="M${n(q.x)},${n(y2 + 4)} l-4.5,-8 l9,0 z" style="fill:var(--dg-cause)"/>`;
    if (l.label) labels += text((p.x + q.x) / 2, (y1 + y2) / 2 + 4, l.label, 11, { anchor: "middle", color: "--dg-cause", halo: true });
  }
  let boxes = "";
  for (const x of nodes) {
    const p = pos.get(x.id)!;
    const h = boxH(x.id);
    boxes += `<rect x="${n(p.x - BOX_W / 2)}" y="${n(p.y - h / 2)}" width="${BOX_W}" height="${h}" rx="9" style="fill:var(${NODE_KIND[x.kind] ?? "--dg-g2"});stroke:var(--dg-line)"/>`;
    boxLines.get(x.id)!.forEach((ln, i) => (boxes += text(p.x, p.y - h / 2 + 21 + i * 17, ln, FS, { anchor: "middle", weight: i === 0 ? 600 : 400 })));
  }
  return svg(W, H, lines + labels + boxes, label);
}

/** 图示的标题 */
export const DIAGRAM_TITLE: Record<string, string> = {
  relations: "人物关系图",
  plot: "故事进展与伏笔",
  tree: "组织与等级",
  family: "家谱",
  route: "地点路线",
  rhythm: "全书节奏",
  compare: "对照",
  causal: "因果与论证",
  concepts: "概念关系",
};

/** 「图解」部分：AI 选的每张图一个 h3 书签，下面一句「帮你看懂什么」 */
export function diagramsHtml(d: Diagrams, chapters: DigestChapter[], parts: PartBand[], label: (idx: number) => string): string {
  return d.chosen
    .map((c) => {
      const body =
        c.kind === "relations"
          ? relationsSvg(d.relations)
          : c.kind === "plot"
            ? plotSvg(d.plot, parts, label)
            : c.kind === "tree"
              ? treeSvg(d.tree, tr("组织与等级"))
              : c.kind === "family"
                ? treeSvg(d.family, tr("家谱"))
                : c.kind === "route"
                  ? routeSvg(d.route, label)
                  : c.kind === "rhythm"
                    ? rhythmSvg(chapters, d.rhythm, parts, label)
                    : c.kind === "compare"
                      ? compareSvg(d.compare, label)
                      : c.kind === "causal"
                        ? graphSvg(d.causal, "layers", tr("因果与论证"))
                        : c.kind === "concepts"
                          ? graphSvg(d.concepts, "circle", tr("概念关系"))
                          : "";
      if (!body) return "";
      const title = c.kind === "compare" && d.compare.title ? d.compare.title : tr(DIAGRAM_TITLE[c.kind] ?? c.kind);
      return `<div class="dg-figure"><h3>${e(title)}</h3>${c.why ? `<p class="dg-figwhy"><b>${tr("帮你看懂")}</b>${e(c.why)}</p>` : ""}${body}</div>`;
    })
    .join("");
}
