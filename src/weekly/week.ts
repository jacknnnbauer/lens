// 阅读周报的「一周」：本地时间周一零点到下周一零点；周号按 ISO 8601。

export interface Week {
  /** 本地周一零点 */
  start: Date;
  /** 下周一零点（不含） */
  end: Date;
  fromDay: string;
  toDay: string;
  fromMs: number;
  toMs: number;
  /** ISO 周所在的年（跨年周可能和日期上的年份不同） */
  year: number;
  week: number;
}

const pad = (n: number) => String(n).padStart(2, "0");

/** 本地日期 YYYY-MM-DD */
export function dayOf(d: Date): string {
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

function isoWeek(monday: Date): { year: number; week: number } {
  // 这周的周四决定属于哪一年
  const thu = new Date(monday.getFullYear(), monday.getMonth(), monday.getDate() + 3);
  const jan1 = new Date(thu.getFullYear(), 0, 1);
  const days = Math.round((thu.getTime() - jan1.getTime()) / 86_400_000);
  return { year: thu.getFullYear(), week: Math.floor(days / 7) + 1 };
}

function fromMonday(start: Date): Week {
  const end = new Date(start.getFullYear(), start.getMonth(), start.getDate() + 7);
  return { start, end, fromDay: dayOf(start), toDay: dayOf(end), fromMs: start.getTime(), toMs: end.getTime(), ...isoWeek(start) };
}

export function weekOf(date: Date): Week {
  const back = (date.getDay() + 6) % 7; // 周一为 0
  return fromMonday(new Date(date.getFullYear(), date.getMonth(), date.getDate() - back));
}

export function shiftWeek(w: Week, n: number): Week {
  return fromMonday(new Date(w.start.getFullYear(), w.start.getMonth(), w.start.getDate() + 7 * n));
}
