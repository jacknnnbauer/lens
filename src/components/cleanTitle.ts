// 浏览器窗口标题清理：只留网页本身的标题。SourceLink 和导出模板共用。

// 浏览器窗口标题的尾巴：「 and 1 more page」「 - Personal」「 - Microsoft Edge」「 - Google Chrome」…
const BROWSER_TAIL = /(\s+and \d+ more pages?)?(\s+-\s+(Personal|Work|个人|工作|InPrivate|[^-]*Profile[^-]*))?\s+-\s+(Microsoft\u200b?\s*Edge|Google Chrome|Mozilla Firefox|Firefox|Opera|Brave|Vivaldi)$/i;
// 「and 1 more page」后面跟着的是浏览器配置名（可能是任意名字），一并去掉
const MORE_PAGES = /\s+(and \d+ more pages?|和另外 \d+ 个页面)(\s+-\s+[^-]+)?$/i;

/** 去掉浏览器加在网页标题后面的部分，只留网页本身的标题 */
export function cleanTitle(t: string): string {
  return t.replace(BROWSER_TAIL, "").replace(MORE_PAGES, "").trim() || t;
}
