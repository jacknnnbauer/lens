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

/** 来源文字：窗口标题；没有标题时用网址的域名 */
export function sourceText(s: Src): string {
  if (s.windowTitle) return s.windowTitle;
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
