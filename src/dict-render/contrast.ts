// 词典配色的对比度修正（SPEC 9.3「主题覆盖层」、15「文字对比度 ≥ 4.5:1」）。
// 词典 CSS 往往把颜色写死成「白底深字」，在石墨主题下看不清。这里只修正不达标的颜色：
// 保留色相与饱和度，只调明度直到达标；深色主题下把词典的浅色底块换成 --raised。

export type RGB = [number, number, number];

export interface Rgba {
  rgb: RGB;
  a: number;
}

const NORMAL_MIN = 4.5;
const LARGE_MIN = 3;
/** 深色主题下，亮度高于此值的词典底色视为「浅色底块」 */
const LIGHT_BG_LUMINANCE = 0.4;

export function parseColor(s: string): Rgba | null {
  const t = s.trim().toLowerCase();
  const hex = /^#([0-9a-f]{3}|[0-9a-f]{6})$/.exec(t);
  if (hex) {
    const h = hex[1].length === 3 ? [...hex[1]].map((c) => c + c).join("") : hex[1];
    return { rgb: [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16)) as RGB, a: 1 };
  }
  const fn = /^rgba?\(([^)]+)\)$/.exec(t);
  if (!fn) return null;
  const parts = fn[1].split(/[\s,/]+/).filter(Boolean);
  if (parts.length < 3) return null;
  const rgb = parts.slice(0, 3).map(Number) as RGB;
  if (rgb.some((n) => Number.isNaN(n))) return null;
  const alpha = parts[3];
  const a = alpha === undefined ? 1 : alpha.endsWith("%") ? parseFloat(alpha) / 100 : parseFloat(alpha);
  return { rgb, a: Number.isNaN(a) ? 1 : a };
}

export function luminance([r, g, b]: RGB): number {
  const lin = (c: number) => {
    const v = c / 255;
    return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

export function contrast(a: RGB, b: RGB): number {
  const [x, y] = [luminance(a), luminance(b)].sort((p, q) => q - p);
  return (x + 0.05) / (y + 0.05);
}

/** 把半透明颜色叠到背景上 */
export function over(c: Rgba, bg: RGB): RGB {
  return c.rgb.map((v, i) => Math.round(v * c.a + bg[i] * (1 - c.a))) as RGB;
}

function toHsl([r, g, b]: RGB): [number, number, number] {
  const [rr, gg, bb] = [r / 255, g / 255, b / 255];
  const max = Math.max(rr, gg, bb);
  const min = Math.min(rr, gg, bb);
  const l = (max + min) / 2;
  if (max === min) return [0, 0, l];
  const d = max - min;
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  const h = max === rr ? (gg - bb) / d + (gg < bb ? 6 : 0) : max === gg ? (bb - rr) / d + 2 : (rr - gg) / d + 4;
  return [h / 6, s, l];
}

function fromHsl([h, s, l]: [number, number, number]): RGB {
  if (s === 0) return [l, l, l].map((v) => Math.round(v * 255)) as RGB;
  const q = l < 0.5 ? l * (1 + s) : l + s - l * s;
  const p = 2 * l - q;
  const hue = (t: number) => {
    const u = t < 0 ? t + 1 : t > 1 ? t - 1 : t;
    if (u < 1 / 6) return p + (q - p) * 6 * u;
    if (u < 1 / 2) return q;
    if (u < 2 / 3) return p + (q - p) * (2 / 3 - u) * 6;
    return p;
  };
  return [hue(h + 1 / 3), hue(h), hue(h - 1 / 3)].map((v) => Math.round(v * 255)) as RGB;
}

/** 对比度不足时，沿明度方向（暗底变亮、亮底变暗）调整到达标；已达标返回 null。 */
export function fixColor(fg: RGB, bg: RGB, min: number): RGB | null {
  if (contrast(fg, bg) >= min) return null;
  const [h, s, l0] = toHsl(fg);
  const up = luminance(bg) < 0.5;
  for (let l = l0; up ? l <= 1 : l >= 0; l += up ? 0.02 : -0.02) {
    const c = fromHsl([h, s, Math.min(1, Math.max(0, l))]);
    if (contrast(c, bg) >= min) return c;
  }
  return up ? [255, 255, 255] : [0, 0, 0];
}

const rgbCss = ([r, g, b]: RGB) => `rgb(${r}, ${g}, ${b})`;

function hasOwnText(el: Element): boolean {
  for (const n of Array.from(el.childNodes)) {
    if (n.nodeType === 3 && (n.textContent ?? "").trim()) return true;
  }
  return false;
}

const SKIP = new Set(["STYLE", "LINK", "SCRIPT", "IMG", "SVG", "BR", "HR"]);

export interface ThemeColors {
  panel: RGB;
  raised: RGB;
  dark: boolean;
}

/**
 * 修正 root 内文字颜色与浅色底块，返回修改的元素数。
 * 先一次性读完计算样式再统一写，避免读写交替反复触发样式重算。
 */
export function adjustContrast(root: ParentNode, theme: ThemeColors): number {
  const els = Array.from(root.querySelectorAll("*")).filter((e) => !SKIP.has(e.tagName.toUpperCase()));
  const effBg = new Map<Element, RGB>();
  const writes: Array<() => void> = [];

  const parentBg = (el: Element): RGB => {
    for (let p = el.parentElement; p; p = p.parentElement) {
      const b = effBg.get(p);
      if (b) return b;
    }
    return theme.panel;
  };

  for (const el of els) {
    const cs = getComputedStyle(el);
    const under = parentBg(el);
    let bg = under;
    const ownBg = parseColor(cs.backgroundColor);
    if (ownBg && ownBg.a > 0.05) {
      const composed = over(ownBg, under);
      if (theme.dark && luminance(composed) > LIGHT_BG_LUMINANCE) {
        bg = theme.raised;
        writes.push(() => (el as HTMLElement).style.setProperty("background-color", "var(--raised)", "important"));
      } else {
        bg = composed;
      }
    }
    effBg.set(el, bg);

    if (!hasOwnText(el)) continue;
    const fg = parseColor(cs.color);
    if (!fg) continue;
    const size = parseFloat(cs.fontSize) || 16;
    const bold = (parseInt(cs.fontWeight, 10) || 400) >= 700;
    const min = size >= 24 || (size >= 18.66 && bold) ? LARGE_MIN : NORMAL_MIN;
    const fixed = fixColor(over(fg, bg), bg, min);
    if (fixed) writes.push(() => (el as HTMLElement).style.setProperty("color", rgbCss(fixed), "important"));
  }

  for (const w of writes) w();
  return writes.length;
}
