// lens:// 资源地址。Windows 上 WebView2 访问自定义协议用 http://lens.localhost/…

import { convertFileSrc } from "@tauri-apps/api/core";

let base: string | null = null;

/** 协议根地址，形如 `http://lens.localhost/`。测试环境没有 Tauri 时用 Windows 的形式。 */
export function lensBase(): string {
  if (base === null) {
    try {
      base = convertFileSrc("", "lens");
    } catch {
      base = "http://lens.localhost/";
    }
    if (!base.endsWith("/")) base += "/";
  }
  return base;
}

/**
 * 词典资源地址：保留路径里的 `/`（CSS 里的相对 url() 才能正确解析），逐段编码。
 * `path` 为词典内路径，`\` 与 `/` 均可。
 */
export function lensUrl(dictId: string, path: string): string {
  const segments = path
    .replace(/\\/g, "/")
    .split("/")
    // 去掉 `..`：浏览器会先规范化 URL，`res/<id>/../x` 会跑到别的词典或协议根下
    .filter((s) => s !== "" && s !== "." && s !== "..");
  return `${lensBase()}res/${encodeURIComponent(dictId)}/${segments.map(encodeURIComponent).join("/")}`;
}

export function isLensUrl(url: string): boolean {
  return url.startsWith(lensBase());
}
