// 语境句的来源：只显示窗口标题（不显示 msedge.exe 这类进程名）；有网页地址时整段可点，打开原网页。

import { openUrl } from "@tauri-apps/plugin-opener";
import { tr } from "../i18n";

interface Src {
  windowTitle: string | null;
  url: string | null;
}

export function canOpen(url: string | null | undefined): url is string {
  return !!url && /^https?:\/\//i.test(url);
}

// 浏览器窗口标题的尾巴：「 and 1 more page」「 - Personal」「 - Microsoft Edge」「 - Google Chrome」…
const BROWSER_TAIL = /(\s+and \d+ more pages?)?(\s+-\s+(Personal|Work|个人|工作|InPrivate|[^-]*Profile[^-]*))?\s+-\s+(Microsoft\u200b?\s*Edge|Google Chrome|Mozilla Firefox|Firefox|Opera|Brave|Vivaldi)$/i;
// 「and 1 more page」后面跟着的是浏览器配置名（可能是任意名字），一并去掉
const MORE_PAGES = /\s+(and \d+ more pages?|和另外 \d+ 个页面)(\s+-\s+[^-]+)?$/i;

/** 去掉浏览器加在网页标题后面的部分，只留网页本身的标题 */
export function cleanTitle(t: string): string {
  return t.replace(BROWSER_TAIL, "").replace(MORE_PAGES, "").trim() || t;
}

/** 来源文字：窗口标题；没有标题时用网址的域名 */
export function sourceText(s: Src): string {
  if (s.windowTitle) return cleanTitle(s.windowTitle);
  if (canOpen(s.url)) {
    try {
      return new URL(s.url).host;
    } catch {
      return s.url;
    }
  }
  return "";
}

export function SourceLink({ src, className }: { src: Src; className?: string }) {
  const text = sourceText(src);
  const url = src.url;
  if (!text) return null;
  if (!canOpen(url)) return <span className={className}>{text}</span>;
  return (
    <a
      href={url}
      className={className}
      title={tr("打开原网页：{url}", { url })}
      onClick={(e) => {
        e.preventDefault();
        void openUrl(url);
      }}
    >
      {text} ↗
    </a>
  );
}
