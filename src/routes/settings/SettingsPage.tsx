// 设置页（SPEC 14、画板 06）：左侧分类、右侧分节，改动立即生效，300ms 防抖写回 settings.json。
// 完整的主题库（11 套内置 + 自定义）在单独的页面（routes/themes）。

import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useCaptureStatus } from "../../components/HotkeyNotice";
import { htmlToMarkdown, sourceLine } from "../../copy/convert";
import { appPaths, type AppPaths, type Copy, type ExportPrefs, type Immersive, type Layout, type LayoutPreset } from "../../ipc/settings";
import { useSettings } from "../../stores/settings";
import { ThemeThumb } from "../../components/ThemeThumb";
import { ACCENTS, allThemes, findTheme, type CustomTheme } from "../../theme/themes";
import { useAppliedTheme } from "../../theme/useApplyTheme";
import { formatHotkey } from "./hotkey";
import { scrollRoot } from "../../windows/main/scroll";
import styles from "./SettingsPage.module.css";
import { tr } from "../../i18n";

const SECTIONS = [
  { id: "look", name: "外观" },
  { id: "layout", name: "布局" },
  { id: "immersive", name: "沉浸模式" },
  { id: "online", name: "在线词典" },
  { id: "copy", name: "复制" },
  { id: "export", name: "导出" },
  { id: "shortcuts", name: "快捷键" },
  { id: "capture", name: "取词" },
  { id: "data", name: "数据" },
  { id: "about", name: "关于" },
] as const;

const PRESETS: Array<{ id: LayoutPreset; name: string; hint: string; regions: Partial<Layout> }> = [
  { id: "three-column", name: "三栏", hint: "导航 + 结果 + 上下文", regions: { showNav: true, showContext: true, showStatusBar: true } },
  { id: "compact", name: "简洁单栏", hint: "搜索框 + 词典卡片流", regions: { showNav: false, showContext: false, showStatusBar: false } },
  { id: "two-column", name: "双栏", hint: "导航 + 结果", regions: { showNav: true, showContext: false, showStatusBar: true } },
];

/** 应用内快捷键（SPEC 9.4），只读说明 */
const SHORTCUTS: Array<[string, string]> = [
  ["Ctrl+K", "聚焦搜索框"],
  ["↑ ↓", "候选 / 义项移动"],
  ["Tab / Shift+Tab", "下一本 / 上一本词典"],
  ["Ctrl+S", "收藏（带语境句）"],
  ["Ctrl+Shift+C", "复制当前词典（默认格式）"],
  ["Ctrl+Shift+A", "合并复制所有词典"],
  ["Ctrl+E", "导出当前词"],
  ["Ctrl+1 / 2 / 3", "切换场景配置组"],
  ["Ctrl+B / Ctrl+H / Ctrl+L", "生词本 / 历史 / 词典库"],
  ["F11 / Esc", "进入 / 退出沉浸模式"],
  ["Ctrl+\\", "显示 / 隐藏左侧导航"],
  ["Ctrl+,", "设置"],
  ["Enter", "弹窗中：展开到主窗口"],
  ["Alt+← / Alt+→", "弹窗中：语境句向前 / 向后多带一句"],
];

const COPY_SAMPLE = `<span class="phon">/juːˈbɪk.wɪ.təs/</span> <i>adj.</i>
<div class="sense">appearing or found everywhere, often at the same time
  <span class="x">Phone screens have become ubiquitous on the morning commute.</span></div>
<img src="https://example.org/commute.png" alt="插图">`;

function Toggle({ label, hint, checked, disabled, onChange }: { label: ReactNode; hint?: string; checked: boolean; disabled?: boolean; onChange: (v: boolean) => void }) {
  return (
    <label className={styles.row}>
      <span>
        {typeof label === "string" ? tr(label) : label}
        {hint && <small>{tr(hint)}</small>}
      </span>
      <input type="checkbox" role="switch" className={styles.switch} checked={checked} disabled={disabled} onChange={(e) => onChange(e.target.checked)} />
    </label>
  );
}

function Select<T extends string | number>({ label, value, options, onChange }: { label: string; value: T; options: Array<[T, string]>; onChange: (v: T) => void }) {
  return (
    <label className={styles.row}>
      <span>{tr(label)}</span>
      <select
        value={String(value)}
        onChange={(e) => {
          const hit = options.find(([v]) => String(v) === e.target.value);
          if (hit) onChange(hit[0]);
        }}
      >
        {options.map(([v, name]) => (
          <option key={String(v)} value={String(v)}>
            {tr(name)}
          </option>
        ))}
      </select>
    </label>
  );
}

function HotkeyField({ value, onSave }: { value: string; onSave: (v: string) => void }) {
  const [recording, setRecording] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const status = useCaptureStatus();
  const btn = useRef<HTMLButtonElement>(null);

  return (
    <div className={styles.hotkey}>
      <button
        ref={btn}
        type="button"
        className={styles.recorder}
        data-recording={recording || undefined}
        aria-describedby="hotkey-status"
        onClick={() => {
          setRecording(true);
          setMessage(null);
        }}
        onBlur={() => setRecording(false)}
        onKeyDown={(e) => {
          if (!recording) return;
          e.preventDefault();
          e.stopPropagation();
          if (e.key === "Escape" && !e.ctrlKey && !e.altKey) {
            setRecording(false);
            return;
          }
          const r = formatHotkey(e);
          if (!r) return;
          if (!r.ok) {
            setMessage(r.reason);
            return;
          }
          setRecording(false);
          setMessage(null);
          if (r.value !== value) onSave(r.value);
        }}
      >
        {recording ? tr("请按下新的组合键…（Esc 取消）") : <kbd>{value}</kbd>}
      </button>
      <span id="hotkey-status" className={styles.hint} role="status">
        {message ??
          (status?.hotkey === value
            ? status.hotkeyRegistered
              ? tr("已生效")
              : tr("未生效：{reason}", { reason: status.hotkeyError ?? tr("注册失败") })
            : "")}
      </span>
    </div>
  );
}

const NO_CUSTOM: CustomTheme[] = [];
/** 外观里直接列出的主题；其余在主题库 */
const QUICK_THEMES = ["paper", "light", "dark"];

export function SettingsPage({ onBack, onOpenThemes }: { onBack: () => void; onOpenThemes: () => void }) {
  const settings = useSettings((s) => s.settings);
  const update = useSettings((s) => s.update);
  const updateNow = useSettings((s) => s.updateNow);
  const appliedTheme = useAppliedTheme((s) => s.theme) ?? "paper";
  const [current, setCurrent] = useState<string>("look");
  const [paths, setPaths] = useState<AppPaths | null>(null);
  const body = useRef<HTMLDivElement>(null);

  useEffect(() => {
    appPaths().then(setPaths, () => setPaths(null));
  }, []);

  // 滚动时高亮当前分节：顶部已越过窗口上方 1/4 处的最后一节；滚到底时取最后一节
  useEffect(() => {
    const root = body.current;
    if (!root) return;
    const onScroll = () => {
      const sections = Array.from(root.querySelectorAll<HTMLElement>("section[id^='set-']"));
      const scroller = scrollRoot();
      const atBottom = scroller.clientHeight + scroller.scrollTop >= scroller.scrollHeight - 4;
      const line = scroller.getBoundingClientRect().top + scroller.clientHeight / 4;
      const hit = atBottom ? sections[sections.length - 1] : sections.filter((s) => s.getBoundingClientRect().top <= line).pop();
      setCurrent((hit ?? sections[0])?.id.replace(/^set-/, "") ?? "look");
    };
    onScroll();
    const scroller = scrollRoot();
    scroller.addEventListener("scroll", onScroll, { passive: true });
    return () => scroller.removeEventListener("scroll", onScroll);
  }, [settings === null]);

  const copy = settings?.copy;
  const mdPreview = useMemo(() => {
    if (!copy) return "";
    const md = htmlToMarkdown(COPY_SAMPLE, { includeExamples: copy.includeExamples, imagesAsLinks: copy.mdImagesAsLinks });
    return `## ubiquitous\n${md}${copy.appendSource ? `\n\n${sourceLine("md", { dict: tr("牛津高阶"), headword: "ubiquitous" })}` : ""}`;
  }, [copy]);

  if (!settings) return null;
  const { appearance: a, layout: l, immersive: im, capture: cap, export: ex } = settings;
  const setA = (p: Partial<typeof a>) => update({ appearance: p });
  const setL = (p: Partial<Layout>) => update({ layout: p });
  const setI = (p: Partial<Immersive>) => update({ immersive: p });
  const setC = (p: Partial<Copy>) => update({ copy: p });
  const setE = (p: Partial<ExportPrefs>) => update({ export: p });
  const customs = settings.customThemes ?? NO_CUSTOM;
  const themes = allThemes(customs);
  const quick = themes.filter((t) => QUICK_THEMES.includes(t.id));
  const lightThemes = themes.filter((t) => !t.dark);
  const darkThemes = themes.filter((t) => t.dark);
  const thumbAccent = a.accent;
  const chosen = findTheme(a.theme, customs);
  const otherChosen = !a.followSystem && chosen && !QUICK_THEMES.includes(chosen.id) ? chosen : null;
  const appliedInfo = findTheme(appliedTheme, customs);
  const themeName = (t: { name: string; custom?: unknown }) => (t.custom ? t.name : tr(t.name));

  return (
    <div className={styles.page}>
      <nav className={styles.nav} aria-label={tr("设置分类")}>
        <button type="button" className={styles.back} onClick={onBack}>
          {tr("← 返回查词")}
        </button>
        {SECTIONS.map((s) => (
          <a
            key={s.id}
            href={`#set-${s.id}`}
            className={styles.navItem}
            aria-current={current === s.id ? "true" : undefined}
            onClick={(e) => {
              e.preventDefault();
              document.getElementById(`set-${s.id}`)?.scrollIntoView({ behavior: "smooth", block: "start" });
            }}
          >
            {tr(s.name)}
          </a>
        ))}
      </nav>

      <div ref={body} className={styles.body}>
        <section id="set-look" className={styles.section} aria-labelledby="h-look">
          <h2 id="h-look">{tr("外观")}</h2>
          <fieldset className={styles.fieldset}>
            <legend>{tr("主题")}</legend>
            <div className={styles.cards4}>
              {quick.map((t) => (
                <label key={t.id} className={styles.choice} data-checked={(!a.followSystem && a.theme === t.id) || undefined}>
                  <ThemeThumb theme={t} accent={thumbAccent} />
                  <span className={styles.choiceLabel}>
                    <input type="radio" name="theme" checked={!a.followSystem && a.theme === t.id} onChange={() => setA({ theme: t.id, followSystem: false })} />
                    {t.dark ? tr("暗色") : tr("亮色")} · {tr(t.name)}
                    {t.id === "paper" && <small>{tr("适合长读")}</small>}
                  </span>
                </label>
              ))}
              <label className={styles.choice} data-checked={a.followSystem || undefined}>
                <span className={styles.split} aria-hidden="true">
                  <ThemeThumb theme={lightThemes.find((t) => t.id === a.lightTheme) ?? lightThemes[0]} accent={thumbAccent} />
                  <ThemeThumb theme={darkThemes.find((t) => t.id === a.darkTheme) ?? darkThemes[0]} accent={thumbAccent} />
                </span>
                <span className={styles.choiceLabel}>
                  <input type="radio" name="theme" checked={a.followSystem} onChange={() => setA({ followSystem: true })} />
                  {tr("跟随系统")}
                </span>
              </label>
            </div>
            {a.followSystem && (
              <p className={styles.inline}>
                {tr("跟随系统时：亮色用")}
                <select aria-label={tr("亮色时使用")} value={a.lightTheme} onChange={(e) => setA({ lightTheme: e.target.value })}>
                  {lightThemes.map((t) => (
                    <option key={t.id} value={t.id}>
                      {themeName(t)}
                    </option>
                  ))}
                </select>
                {tr("，暗色用")}
                <select aria-label={tr("暗色时使用")} value={a.darkTheme} onChange={(e) => setA({ darkTheme: e.target.value })}>
                  {darkThemes.map((t) => (
                    <option key={t.id} value={t.id}>
                      {themeName(t)}
                    </option>
                  ))}
                </select>
                {tr("。")}
              </p>
            )}
            <div className={styles.moreThemes}>
              <span>
                {otherChosen ? tr("正在使用主题库中的「{name}」", { name: themeName(otherChosen) }) : tr("主题库还有宣纸、护眼棕、霜蓝、墨水屏、午夜蓝、苔原、暮紫、高对比等主题，也可以自己调色、导入导出。")}
              </span>
              <button type="button" className={styles.linkBtn} onClick={onOpenThemes}>
                {tr("打开主题库 · {n} 套", { n: themes.length })}
              </button>
            </div>
          </fieldset>

          <fieldset className={styles.fieldset}>
            <legend>{tr("强调色")}</legend>
            <div className={styles.accents}>
              {ACCENTS.map((ac) => (
                <label key={ac.id} className={styles.accent}>
                  <input type="radio" name="accent" className="sr-only" checked={a.accent === ac.id} onChange={() => setA({ accent: ac.id })} />
                  <span className={styles.dot} data-theme={appliedInfo?.accentable ? appliedTheme : appliedInfo?.dark ? "dark" : "paper"} data-accent={ac.id} aria-hidden="true" />
                  {tr(ac.name)}
                </label>
              ))}
            </div>
            <p className={styles.hint}>
              {appliedInfo && !appliedInfo.accentable
                ? tr("当前主题「{name}」自带强调色，这里的选择只对纸本、冷白、石墨生效。", { name: themeName(appliedInfo) })
                : tr("每种强调色在亮色、暗色主题下各有一套值，保证文字对比度。")}
            </p>
          </fieldset>

          <div className={styles.grid2}>
            <label className={styles.stack}>
              <span>
                {tr("释义字号")} <code>{a.glossFontSize}px</code>
              </span>
              <input type="range" min={13} max={22} value={a.glossFontSize} onChange={(e) => setA({ glossFontSize: Number(e.target.value) })} />
            </label>
            <label className={styles.stack}>
              <span>{tr("释义字体")}</span>
              <select value={a.glossFont} onChange={(e) => setA({ glossFont: e.target.value as "serif" | "sans" })}>
                <option value="serif">{tr("衬线 · Source Serif 4 + 思源宋体")}</option>
                <option value="sans">{tr("无衬线 · IBM Plex Sans + 微软雅黑")}</option>
              </select>
            </label>
            <label className={styles.stack}>
              <span>{tr("界面语言")} · Language</span>
              {/* 语言名用各自的写法，不翻译 */}
              <select value={a.language} onChange={(e) => setA({ language: e.target.value as "zh" | "en" })}>
                <option value="zh">中文</option>
                <option value="en">English</option>
              </select>
            </label>
          </div>
        </section>

        <section id="set-layout" className={styles.section} aria-labelledby="h-layout">
          <h2 id="h-layout">{tr("布局")}</h2>
          <fieldset className={styles.fieldset}>
            <legend className="sr-only">{tr("布局预设")}</legend>
            <div className={styles.cards3}>
              {PRESETS.map((p) => (
                <label key={p.id} className={styles.choice} data-checked={l.preset === p.id || undefined}>
                  <span className={styles.layoutThumb} data-preset={p.id} aria-hidden="true">
                    <i />
                    <b>
                      <em />
                      <u />
                      <u />
                    </b>
                    <i />
                  </span>
                  <span className={styles.choiceLabel}>
                    <input type="radio" name="layout" checked={l.preset === p.id} onChange={() => setL({ preset: p.id, ...p.regions })} />
                    {tr(p.name)}
                    <small>{tr(p.hint)}</small>
                  </span>
                </label>
              ))}
            </div>
          </fieldset>
          <div className={styles.grid2}>
            <Toggle label={tr("显示左侧导航")} checked={l.showNav} onChange={(v) => setL({ showNav: v })} />
            <Toggle label={tr("显示右侧上下文栏")} checked={l.showContext} onChange={(v) => setL({ showContext: v })} />
            <Toggle label={tr("显示状态栏")} checked={l.showStatusBar} onChange={(v) => setL({ showStatusBar: v })} />
            <Toggle label={tr("词典跳转条")} checked={l.showJumpBar} onChange={(v) => setL({ showJumpBar: v })} />
            <Toggle
              label={tr("义项图谱（实验）")}
              hint={tr("查词结果上方把几部词典的义项对齐，显示共识与差异")}
              checked={l.showSenseAtlas}
              onChange={(v) => setL({ showSenseAtlas: v })}
            />
            <Toggle label={tr("启动时回到简洁单栏")} hint={tr("不管上次用的是哪种布局")} checked={l.startCompact} onChange={(v) => setL({ startCompact: v })} />
          </div>
        </section>

        <section id="set-immersive" className={styles.section} aria-labelledby="h-immersive">
          <div className={styles.titleRow}>
            <h2 id="h-immersive">{tr("沉浸模式")}</h2>
            <span className={styles.hint}>{tr("只留下单词和释义；按 F11 进入，Esc 或 F11 退出")}</span>
          </div>
          <div className={styles.grid2}>
            <Toggle label={tr("隐藏标题栏")} checked={im.hideTitleBar} onChange={(v) => setI({ hideTitleBar: v, borderless: v })} />
            <Toggle label={tr("隐藏状态栏")} checked={im.hideStatusBar} onChange={(v) => setI({ hideStatusBar: v })} />
            <Toggle label={tr("隐藏左侧导航与右侧栏")} checked={im.hideSidebars} onChange={(v) => setI({ hideSidebars: v })} />
            <Toggle label={tr("隐藏跳转条与复制按钮")} checked={im.hideCardChrome} onChange={(v) => setI({ hideCardChrome: v })} />
            <Toggle label={tr("鼠标移到顶部时浮现工具栏")} checked={im.revealOnHover} onChange={(v) => setI({ revealOnHover: v })} />
            <Select
              label={tr("窗口方式")}
              value={im.fullscreen ? "full" : "window"}
              options={[
                ["window", "保持窗口大小"],
                ["full", "全屏"],
              ]}
              onChange={(v) => setI({ fullscreen: v === "full" })}
            />
            <Select
              label={tr("正文宽度")}
              value={im.width}
              options={[
                [720, "720 px（推荐）"],
                [600, "600 px"],
                [900, "900 px"],
              ]}
              onChange={(v) => setI({ width: v })}
            />
            <Select
              label={tr("沉浸时使用主题")}
              value={im.theme ?? ""}
              options={[["", "跟随当前主题"], ...themes.map((t) => [t.id, t.name] as [string, string])]}
              onChange={(v) => setI({ theme: v || null })}
            />
            <Toggle label={tr("启动时直接进入沉浸模式")} checked={im.onStartup} onChange={(v) => setI({ onStartup: v })} />
          </div>
          <p className={styles.hint}>{tr("只想去掉状态栏，不需要完整的沉浸模式？用上面「布局」里的「显示状态栏」开关即可。")}</p>
        </section>

        <section id="set-online" className={styles.section} aria-labelledby="h-online">
          <h2 id="h-online">{tr("在线词典")}</h2>
          <fieldset className={styles.fieldset}>
            <legend>{tr("默认查询时机（可在词典库里给每部词典单独设置）")}</legend>
            {(
              [
                ["always", "每次都查", "结果最全，会多发网络请求"],
                ["localMiss", "本地词典没有结果时才查", "推荐：快，而且尽量少联网"],
                ["manual", "手动点击才查", "卡片上显示「查询」按钮"],
              ] as const
            ).map(([id, name, hint]) => (
              <label key={id} className={styles.radioRow}>
                <input type="radio" name="online-trigger" checked={settings.online.trigger === id} onChange={() => update({ online: { trigger: id } })} />
                <span>
                  {tr(name)}
                  <small>{tr(hint)}</small>
                </span>
              </label>
            ))}
          </fieldset>
          <div className={styles.grid2}>
            <Select
              label={tr("请求超时")}
              value={settings.online.timeoutMs}
              options={[
                [3000, "3 秒"],
                [5000, "5 秒"],
                [10000, "10 秒"],
              ]}
              onChange={(v) => update({ online: { timeoutMs: v } })}
            />
            <Select
              label={tr("结果缓存")}
              value={settings.online.cacheDays}
              options={[
                [7, "7 天"],
                [30, "30 天"],
                [-1, "永久"],
                [0, "不缓存"],
              ]}
              onChange={(v) => update({ online: { cacheDays: v } })}
            />
          </div>
          <p className={styles.hint}>{tr("超时不会拖慢本地词典的结果。联网失败时显示过期的缓存，并标注获取时间。添加在线词典、填写 API Key 在「词典库」（Ctrl+L）。")}</p>
        </section>

        <section id="set-copy" className={styles.section} aria-labelledby="h-copy">
          <h2 id="h-copy">{tr("复制")}</h2>
          <fieldset className={styles.fieldset}>
            <legend>{tr("点击复制按钮时的默认格式")}</legend>
            <div className={styles.segmented}>
              {(
                [
                  ["md", "Markdown"],
                  ["html", "HTML"],
                  ["text", "纯文本"],
                ] as const
              ).map(([id, name]) => (
                <label key={id} data-checked={settings.copy.defaultFormat === id || undefined}>
                  <input type="radio" name="copy-format" className="sr-only" checked={settings.copy.defaultFormat === id} onChange={() => setC({ defaultFormat: id })} />
                  {tr(name)}
                </label>
              ))}
            </div>
          </fieldset>
          <div className={styles.grid2}>
            <div>
              <Toggle label={tr("末尾附来源（词典名 · 词条）")} checked={settings.copy.appendSource} onChange={(v) => setC({ appendSource: v })} />
              <Toggle label={tr("包含例句")} checked={settings.copy.includeExamples} onChange={(v) => setC({ includeExamples: v })} />
              <Toggle label={tr("HTML 内联词典样式")} hint={tr("粘贴到 Word 保留颜色与粗斜体")} checked={settings.copy.inlineHtmlStyles} onChange={(v) => setC({ inlineHtmlStyles: v })} />
              <Toggle label={tr("Markdown 图片转为链接")} checked={settings.copy.mdImagesAsLinks} onChange={(v) => setC({ mdImagesAsLinks: v })} />
            </div>
            <div className={styles.stack}>
              <span className={styles.hint}>{tr("Markdown 预览")}</span>
              <pre className={styles.preview}>{mdPreview}</pre>
            </div>
          </div>
        </section>

        <section id="set-export" className={styles.section} aria-labelledby="h-export">
          <h2 id="h-export">{tr("导出")}</h2>
          <p className={styles.hint}>{tr("导出对话框（Ctrl+E）的默认值；在对话框里改过后会记住上一次的选择。")}</p>
          <div className={styles.grid2}>
            <Select
              label={tr("版式")}
              value={ex.layout}
              options={[
                ["cards", "卡片式"],
                ["document", "文档式"],
              ]}
              onChange={(v) => setE({ layout: v })}
            />
            <Select
              label={tr("样式")}
              value={ex.style}
              options={[
                ["paperWhite", "纸本（白底打印版）"],
                ["followTheme", "跟随当前主题"],
                ["inkSaving", "黑白省墨"],
              ]}
              onChange={(v) => setE({ style: v })}
            />
            <Select
              label={tr("纸张")}
              value={ex.paper}
              options={[
                ["a4", "A4 纵向"],
                ["letter", "Letter 纵向"],
              ]}
              onChange={(v) => setE({ paper: v })}
            />
            <div className={styles.row}>
              <span>
                {tr("保存位置")}
                <small className={styles.path}>{ex.dir ?? tr("上次导出的位置（尚未导出过）")}</small>
              </span>
              <span className={styles.buttons}>
                <button
                  type="button"
                  className={styles.ghost}
                  onClick={async () => {
                    const dir = await openDialog({ directory: true, title: tr("选择导出文件夹"), defaultPath: ex.dir ?? undefined });
                    if (typeof dir === "string") setE({ dir });
                  }}
                >
                  {tr("选择…")}
                </button>
              </span>
            </div>
          </div>
        </section>

        <section id="set-shortcuts" className={styles.section} aria-labelledby="h-shortcuts">
          <h2 id="h-shortcuts">{tr("快捷键")}</h2>
          <div className={styles.row}>
            <span>
              {tr("全局划词快捷键")}
              <small>{tr("在任何程序里选中文字后按下，弹出查词窗口；点击后按下新组合键即可更换")}</small>
            </span>
            <HotkeyField value={cap.hotkey} onSave={(v) => void updateNow({ capture: { hotkey: v } })} />
          </div>
          <table className={styles.keys}>
            <caption className="sr-only">{tr("应用内快捷键")}</caption>
            <tbody>
              {SHORTCUTS.map(([k, what]) => (
                <tr key={k}>
                  <th scope="row">
                    <kbd>{k}</kbd>
                  </th>
                  <td>{tr(what)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </section>

        <section id="set-capture" className={styles.section} aria-labelledby="h-capture">
          <h2 id="h-capture">{tr("取词")}</h2>
          <div className={styles.grid2}>
            <Toggle label={tr("Ctrl+C+C 划词")} hint={tr("连按两次复制即查词，任何程序都能用")} checked={cap.doubleCopy} onChange={(v) => void updateNow({ capture: { doubleCopy: v } })} />
            <Select
              label={tr("取词等待")}
              value={cap.timeoutMs}
              options={[
                [100, "100 ms（更快弹出）"],
                [150, "150 ms（推荐）"],
                [300, "300 ms"],
                [500, "500 ms（较慢的程序）"],
              ]}
              onChange={(v) => update({ capture: { timeoutMs: v } })}
            />
          </div>
          <p className={styles.hint}>{tr("取词超过等待时间仍未拿到语境句时先弹窗，用剪贴板里的文字查词。")}</p>
        </section>

        <section id="set-data" className={styles.section} aria-labelledby="h-data">
          <h2 id="h-data">{tr("数据")}</h2>
          {paths && (
            <>
              <div className={styles.row}>
                <span>
                  {tr("数据文件夹")}
                  <small className={styles.path}>{paths.data}</small>
                </span>
                <button type="button" className={styles.ghost} onClick={() => void revealItemInDir(paths.settings)}>
                  {tr("在资源管理器中显示")}
                </button>
              </div>
              <div className={styles.row}>
                <span>
                  {tr("日志")}
                  <small className={styles.path}>{paths.logs}</small>
                </span>
                <button type="button" className={styles.ghost} onClick={() => void revealItemInDir(paths.logs)}>
                  {tr("在资源管理器中显示")}
                </button>
              </div>
            </>
          )}
          <p className={styles.hint}>{tr("查词历史、生词本、词典索引都保存在本机，不会上传。清空历史在「历史」页。")}</p>
        </section>

        <section id="set-about" className={styles.section} aria-labelledby="h-about">
          <h2 id="h-about">{tr("关于")}</h2>
          <p className={styles.about}>
            词镜 Lens {paths ? `v${paths.version}` : ""}
            <br />
            {tr("离线多词典查词、划词取句、复制与导出。软件不附带任何词典；释义内容版权归各词典所有，仅供个人学习使用。")}
          </p>
        </section>
      </div>
    </div>
  );
}
