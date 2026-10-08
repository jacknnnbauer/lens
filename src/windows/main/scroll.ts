// 主窗口的滚动容器在自绘标题栏下方（不是 document），需要滚动的地方都通过这里拿。

export const SCROLL_ID = "lens-scroll";

export function scrollRoot(): HTMLElement {
  return document.getElementById(SCROLL_ID) ?? (document.scrollingElement as HTMLElement | null) ?? document.documentElement;
}
