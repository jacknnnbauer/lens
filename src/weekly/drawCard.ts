// 在 Canvas 上画周报卡片（1080×1620）：配色、字号照样稿 design/prototypes/weekly-card.html。

import { tr } from "../i18n";
import type { Week } from "./week";
import { CONTENT, FONT, FOOT_GAP, NOTE_FONT, NOTE_LINE, PAD, QUOTE_FONT, QUOTE_LINE, W, WORDS_FONT, type CardLayout } from "./layout";

const C = {
  bg: "#f6f1e7",
  ink: "#2b2620",
  accent: "#b9562f",
  grow: "#3f6b4a",
  label: "#8a7f70",
  faint: "#a99d8c",
  empty: "#d9cfbf",
  quoteBg: "#fffdf8",
  noteInk: "#6b6255",
};

/** 卡片在 Canvas 里怎么量字（给 layoutCard 用） */
export function canvasMeasure(ctx: CanvasRenderingContext2D) {
  return (text: string, font: string) => {
    ctx.font = font;
    return ctx.measureText(text).width;
  };
}

function text(ctx: CanvasRenderingContext2D, s: string, x: number, y: number, font: string, color: string, spacing = 0) {
  ctx.font = font;
  ctx.fillStyle = color;
  if (spacing && "letterSpacing" in ctx) (ctx as unknown as { letterSpacing: string }).letterSpacing = `${spacing}px`;
  ctx.fillText(s, x, y);
  if (spacing && "letterSpacing" in ctx) (ctx as unknown as { letterSpacing: string }).letterSpacing = "0px";
  ctx.font = font;
  return ctx.measureText(s).width + s.length * spacing;
}

/** 大数字后面跟小单位：3 小时 20 分 */
function bigNumber(ctx: CanvasRenderingContext2D, s: string, x: number, y: number) {
  let cx = x;
  for (const part of s.split(/(\d[\d,]*)/).filter(Boolean)) {
    const isNum = /^\d/.test(part);
    cx += text(ctx, isNum ? part : part.trim(), cx, y, isNum ? `700 96px ${FONT.number}` : `40px ${FONT.serif}`, C.ink);
    if (!isNum) cx += 8;
  }
}

function roundTop(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number) {
  ctx.beginPath();
  ctx.moveTo(x, y + h);
  ctx.lineTo(x, y + r);
  ctx.quadraticCurveTo(x, y, x + r, y);
  ctx.lineTo(x + w - r, y);
  ctx.quadraticCurveTo(x + w, y, x + w, y + r);
  ctx.lineTo(x + w, y + h);
  ctx.closePath();
  ctx.fill();
}

export function drawCard(ctx: CanvasRenderingContext2D, layout: CardLayout, week: Week) {
  ctx.textBaseline = "alphabetic";
  const H = layout.height;
  const FOOT_Y = H - FOOT_GAP;
  ctx.fillStyle = C.bg;
  ctx.fillRect(0, 0, W, H);

  // 页眉
  const end = new Date(week.end.getTime() - 86_400_000);
  const md = (d: Date) => `${d.getMonth() + 1}.${d.getDate()}`;
  text(ctx, tr("{year} · 第 {week} 周 · {from} – {to}", { year: week.year, week: week.week, from: md(week.start), to: md(end) }), PAD, 124, `28px ${FONT.sans}`, C.label, 6);
  text(ctx, tr("我的阅读周报"), PAD, 236, `700 76px ${FONT.serif}`, C.ink, 2);
  ctx.fillStyle = C.ink;
  ctx.globalAlpha = 0.85;
  ctx.fillRect(PAD, 298, CONTENT, 2);
  ctx.globalAlpha = 1;

  const label = (s: string, x: number, y: number) => text(ctx, s, x, y, `26px ${FONT.sans}`, C.label, 2);

  if (layout.time) {
    const t = layout.time;
    label(tr("阅读时长"), PAD, t.top + 26);
    bigNumber(ctx, t.text.replace(/\s+/g, " "), PAD, t.top + 128);
    // 7 天柱状
    const bw = 48;
    const gap = 14;
    const bh = 90;
    const base = t.top + 252;
    const max = Math.max(...t.daySeconds, 1);
    t.daySeconds.forEach((s, i) => {
      const x = PAD + i * (bw + gap);
      ctx.fillStyle = s > 0 ? C.accent : C.empty;
      roundTop(ctx, x, s > 0 ? base - Math.max(8, (s / max) * bh) : base - 6, bw, s > 0 ? Math.max(8, (s / max) * bh) : 6, 4);
      ctx.textAlign = "center";
      text(ctx, tr(["一", "二", "三", "四", "五", "六", "日"][i]), x + bw / 2, base + 28, `20px ${FONT.sans}`, C.faint);
      ctx.textAlign = "left";
    });
    text(ctx, t.sub, PAD, base + 70, `24px ${FONT.sans}`, C.label);
  }

  if (layout.vocab) {
    const v = layout.vocab;
    label(tr("词汇量"), v.x, v.top + 26);
    text(ctx, v.value.toLocaleString(), v.x, v.top + 128, `700 96px ${FONT.number}`, C.ink);
    let sx = v.x;
    if (v.delta !== null) {
      const sign = v.delta >= 0 ? "↑ " : "↓ ";
      sx += text(ctx, `${sign}${Math.abs(v.delta).toLocaleString()}`, sx, v.top + 172, `24px ${FONT.sans}`, v.delta >= 0 ? C.grow : C.label);
      text(ctx, ` · ${tr("上周 {n}", { n: (v.previous ?? 0).toLocaleString() })}`, sx, v.top + 172, `24px ${FONT.sans}`, C.label);
    }
    // 折线：本周有快照的那几天
    const pts = v.points;
    const lw = 420;
    const top = v.top + 200;
    const h = 100;
    const lo = Math.min(...pts);
    const hi = Math.max(...pts);
    const xy = pts.map((p, i) => [v.x + (pts.length === 1 ? lw : (i / (pts.length - 1)) * lw), top + h - (hi === lo ? h / 2 : ((p - lo) / (hi - lo)) * h)] as const);
    ctx.strokeStyle = C.grow;
    ctx.lineWidth = 5;
    ctx.lineJoin = "round";
    ctx.lineCap = "round";
    ctx.beginPath();
    xy.forEach(([x, y], i) => (i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y)));
    ctx.stroke();
    const [lx, ly] = xy[xy.length - 1];
    ctx.fillStyle = C.grow;
    ctx.beginPath();
    ctx.arc(lx, ly, 8, 0, Math.PI * 2);
    ctx.fill();
  }

  if (layout.words) {
    const w = layout.words;
    label(tr("新收生词"), PAD, w.top + 26);
    bigNumber(ctx, tr("{n} 个", { n: w.count }), PAD, w.top + 128);
    text(ctx, w.samples, PAD, w.top + 186, WORDS_FONT, C.noteInk);
  }

  if (layout.quote) {
    const q = layout.quote;
    ctx.fillStyle = C.quoteBg;
    ctx.fillRect(PAD, q.top, CONTENT, q.height);
    ctx.fillStyle = C.accent;
    ctx.fillRect(PAD, q.top, 8, q.height);
    const x = PAD + 8 + 56;
    let y = q.top + 48 + 44;
    for (const line of q.lines) {
      text(ctx, line, x, y, QUOTE_FONT, C.ink);
      y += QUOTE_LINE;
    }
    if (q.noteLines.length) {
      y += 8;
      for (const line of q.noteLines) {
        text(ctx, line, x, y, NOTE_FONT, C.noteInk);
        y += NOTE_LINE;
      }
    }
    text(ctx, q.src, x, q.top + q.height - 44, `24px ${FONT.sans}`, C.label);
  }

  text(ctx, tr("词镜 Lens"), PAD, FOOT_Y, `22px ${FONT.sans}`, C.faint);
  ctx.textAlign = "right";
  text(ctx, tr("读英文原著，查词不离页"), W - PAD, FOOT_Y, `22px ${FONT.sans}`, C.faint);
  ctx.textAlign = "left";
}
