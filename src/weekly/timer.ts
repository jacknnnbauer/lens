// 阅读计时：每秒 tick 一次；只有页面在前台、窗口有焦点，而且最近 60 秒内有过操作时才算。
// 攒满 30 秒写一次（flush），停下时把剩下的也写掉。读过的词数由阅读页按位置前进量估算后 addWords。

export const IDLE_MS = 60_000;
export const FLUSH_SECONDS = 30;

export class ReadingTimer {
  private lastActivity = -Infinity;
  private active = true;
  private stopped = false;
  private seconds = 0;
  private words = 0;

  constructor(
    private readonly now: () => number,
    private readonly flush: (seconds: number, words: number) => void,
  ) {}

  /** 鼠标移动、滚动、点击、按键 */
  activity() {
    this.lastActivity = this.now();
  }

  /** 页面可见且窗口有焦点 */
  setActive(on: boolean) {
    this.active = on;
  }

  addWords(n: number) {
    if (!this.stopped && n > 0) this.words += Math.round(n);
  }

  tick() {
    if (this.stopped || !this.active || this.now() - this.lastActivity > IDLE_MS) return;
    this.seconds += 1;
    if (this.seconds >= FLUSH_SECONDS) this.write();
  }

  stop() {
    if (this.stopped) return;
    this.write();
    this.stopped = true;
  }

  private write() {
    if (this.seconds === 0 && this.words === 0) return;
    this.flush(this.seconds, this.words);
    this.seconds = 0;
    this.words = 0;
  }
}
