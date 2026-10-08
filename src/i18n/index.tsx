// 界面语言（设置 › 外观 › 界面语言）：中文 / English。
//
// 源码里直接写中文，用 tr("中文") 包起来（不叫 t：组件里常有局部变量 t，会遮蔽）；中文本身就是键，英文表在 en.ts。
// 切换语言时整棵界面重新挂载（见 LangRoot），所以模块级常量里的中文在渲染处 tr(x) 即可。
// 插值用 {name}：tr("已选 {n} 个词", { n: 3 })。

import { Fragment, type ReactNode } from "react";
import { useSettings } from "../stores/settings";
import { en } from "./en";

export type Lang = "zh" | "en";

let current: Lang = "zh";

export function setLang(lang: Lang) {
  current = lang;
  document.documentElement.lang = lang === "en" ? "en" : "zh-CN";
}

export function getLang(): Lang {
  return current;
}

export function isLang(v: unknown): v is Lang {
  return v === "zh" || v === "en";
}

export function tr(zh: string, vars?: Record<string, string | number>): string {
  let s = current === "en" ? (en[zh] ?? zh) : zh;
  if (vars) s = s.replace(/\{(\w+)\}/g, (m: string, k: string) => (k in vars ? String(vars[k]) : m));
  return s;
}

/** 按设置里的语言渲染；语言变了就整棵重新挂载，所有 tr() 重新取值。 */
export function LangRoot({ children }: { children: ReactNode }) {
  const wanted = useSettings((s) => s.settings?.appearance.language);
  const lang: Lang = isLang(wanted) ? wanted : "zh";
  if (lang !== current) setLang(lang);
  return <Fragment key={lang}>{children}</Fragment>;
}
