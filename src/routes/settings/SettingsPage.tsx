// 设置页（SPEC 14、画板 06）：左侧分类、右侧分节，改动立即生效，300ms 防抖写回 settings.json。
// 完整的主题库（15 套内置 + 自定义）在单独的页面（routes/themes）。

import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { PanelSettings } from "./PanelSettings";
import { ProfileSettings } from "./ProfileSettings";
import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
import { Fragment, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useCaptureStatus } from "../../components/HotkeyNotice";
import { htmlToMarkdown, sourceLine } from "../../copy/convert";
import { aiKeyStatus, aiSetKey, aiTest, aiUsage, aiUsageClear, cocaImport, cocaStatus, type CocaStatus, type ModelUsage, type UsageSummary } from "../../ipc/ai";
import { appPaths, checkUpdate, type AiPrefs, type AppPaths, type UpdateInfo, type Copy, type ExportPrefs, type Immersive, type Layout, type LayoutPreset } from "../../ipc/settings";
import { useSettings } from "../../stores/settings";
import { exportStyleChoices, PAPER_CHOICES } from "../../components/ExportStyleOptions";
import type { ExportStyle, Paper } from "../../export-templates/template";
import { ThemeThumb } from "../../components/ThemeThumb";
import { ACCENTS, allThemes, findTheme, type CustomTheme } from "../../theme/themes";
import { useAppliedTheme } from "../../theme/useApplyTheme";
import { formatHotkey } from "./hotkey";
import { AppKeys } from "./AppKeys";
import { scrollRoot } from "../../windows/main/scroll";
import styles from "./SettingsPage.module.css";
import { tr } from "../../i18n";

const SECTIONS = [
  { id: "general", name: "常规" },
  { id: "look", name: "外观" },
  { id: "layout", name: "布局" },
  { id: "immersive", name: "沉浸模式" },
  { id: "profiles", name: "场景配置组" },
  { id: "online", name: "在线词典" },
  { id: "ai", name: "AI 例句" },
  { id: "copy", name: "复制" },
  { id: "export", name: "导出" },
  { id: "study", name: "全书预习包" },
  { id: "shortcuts", name: "快捷键" },
  { id: "capture", name: "取词" },
  { id: "data", name: "数据" },
  { id: "about", name: "关于" },
] as const;

const PRESETS: Array<{ id: LayoutPreset; name: string; hint: string; regions: Partial<Layout> }> = [
  { id: "compact", name: "简洁单栏", hint: "搜索框 + 词典卡片流", regions: { showNav: false, showContext: false, showStatusBar: false } },
  { id: "two-column", name: "双栏", hint: "导航 + 结果", regions: { showNav: true, showContext: false, showStatusBar: true } },
  { id: "three-column", name: "三栏", hint: "导航 + 结果 + 上下文", regions: { showNav: true, showContext: true, showStatusBar: true } },
];

/** 固定的应用内快捷键（SPEC 9.4），只读说明；可改的见 src/keys.ts 与 AppKeys */
const SHORTCUTS: Array<[string, string]> = [
  ["↑ ↓", "候选 / 义项移动"],
  ["Tab / Shift+Tab", "下一本 / 上一本词典"],
  ["Ctrl+1 / 2 / 3", "切换场景配置组"],
  ["F11 / Esc", "进入 / 退出沉浸模式"],
  ["鼠标侧键 · Alt+← / Alt+→", "主窗口：后退 / 前进（回到刚才查的词或页面）"],
  ["Enter", "查词小窗：展开到主窗口（改过词时先在小窗里查）"],
  ["Tab / Shift+Tab", "查词小窗：下一本 / 上一本词典"],
  ["Alt+← / Alt+→", "查词小窗：语境句向前 / 向后多带一句"],
  ["Esc", "查词小窗：关闭"],
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

/** 设置 › AI 例句：COCA 词频表导入、Claude API Key、模型 */
function AiSettings() {
  const ai = useSettings((s) => s.settings?.ai);
  const update = useSettings((s) => s.update);
  const [coca, setCoca] = useState<CocaStatus | null>(null);
  const [hasKey, setHasKey] = useState(false);
  const provider = ai?.provider ?? "claude";
  const deepseek = provider === "deepseek";
  const [key, setKey] = useState("");
  const [message, setMessage] = useState<{ text: string; error?: boolean } | null>(null);

  useEffect(() => {
    cocaStatus().then(setCoca, () => setCoca(null));
  }, []);
  useEffect(() => {
    setKey("");
    aiKeyStatus(provider).then(setHasKey, () => setHasKey(false));
  }, [provider]);

  if (!ai) return null;
  const setAi = (p: Partial<AiPrefs>) => update({ ai: p });

  const importCoca = async () => {
    const path = await openDialog({ multiple: false, title: tr("选择 COCA 词表"), filters: [{ name: "JSON", extensions: ["json"] }] });
    if (typeof path !== "string") return;
    try {
      const st = await cocaImport(path);
      setCoca(st);
      setMessage({ text: tr("已导入 COCA 词频表：{n} 个词条", { n: st.entries.toLocaleString() }) });
    } catch (e) {
      setMessage({ text: String(e), error: true });
    }
  };

  const [testing, setTesting] = useState(false);
  const testConnection = async () => {
    setTesting(true);
    setMessage({ text: tr("正在测试连接…") });
    try {
      const r = await aiTest();
      setMessage({ text: tr("连接正常：{provider} · {model}，用时 {s} 秒", { provider: r.provider, model: r.model, s: (r.ms / 1000).toFixed(1) }) });
    } catch (e) {
      setMessage({ text: tr("连接失败：{msg}", { msg: String(e) }), error: true });
    } finally {
      setTesting(false);
    }
  };

  const saveKey = async (value: string | null) => {
    try {
      setHasKey(await aiSetKey(provider, value));
      setKey("");
      setMessage({ text: value ? tr("API Key 已保存到 Windows 凭据管理器") : tr("已删除 API Key") });
    } catch (e) {
      setMessage({ text: String(e), error: true });
    }
  };

  return (
    <section id="set-ai" className={styles.section} aria-labelledby="h-ai">
      <h2 id="h-ai">{tr("AI 例句")}</h2>
      <p className={styles.hint}>{tr("右侧栏显示当前词的 COCA 词频、语体与搭配，并可用 Claude 或 DeepSeek 按这些真实搭配生成中英双语例句。")}</p>

      <div className={styles.grid2}>
        <Select
          label={tr("AI 服务")}
          value={provider}
          options={[
            ["claude", "Claude（Anthropic）"],
            ["deepseek", "DeepSeek（深度求索，国内可直连）"],
          ]}
          onChange={(v) => setAi({ provider: v })}
        />
      </div>

      <div className={styles.row}>
        <span>
          {tr("COCA 2 万词频表")}
          <small>{coca?.loaded ? tr("已导入 · {n} 个词条", { n: coca.entries.toLocaleString() }) : tr("未导入 · 选择由词频表 PDF 解析得到的 coca20k.json")}</small>
        </span>
        <button type="button" className={styles.ghost} onClick={() => void importCoca()}>
          {coca?.loaded ? tr("重新导入…") : tr("导入…")}
        </button>
      </div>

      <div className={styles.row}>
        <span>
          {deepseek ? tr("DeepSeek API Key") : tr("Claude API Key")}
          <small>
            {hasKey
              ? tr("已保存（存在 Windows 凭据管理器，不写进设置文件）")
              : deepseek
                ? tr("在 platform.deepseek.com 创建；按用量计费")
                : tr("在 console.anthropic.com 创建；按用量计费")}
          </small>
        </span>
        <span className={styles.keyRow}>
          <input
            type="password"
            value={key}
            placeholder={hasKey ? "••••••••" : deepseek ? "sk-…" : "sk-ant-…"}
            aria-label={deepseek ? tr("DeepSeek API Key") : tr("Claude API Key")}
            autoComplete="off"
            spellCheck={false}
            onChange={(e) => setKey(e.target.value)}
          />
          <button type="button" className={styles.ghost} disabled={!key.trim()} onClick={() => void saveKey(key)}>
            {tr("保存")}
          </button>
          {hasKey && (
            <button type="button" className={styles.ghost} disabled={testing} onClick={() => void testConnection()} title={tr("用当前模型发一句最短的请求，检查网络、API Key 和模型名")}>
              {tr("测试连接")}
            </button>
          )}
          {hasKey && (
            <button type="button" className={styles.ghost} onClick={() => void saveKey(null)}>
              {tr("删除")}
            </button>
          )}
        </span>
      </div>

      <div className={styles.grid2}>
        {deepseek ? (
          <Select
            label={tr("模型")}
            value={ai.deepseekModel}
            options={[
              ["deepseek-flash", "DeepSeek Flash（默认，快、便宜）"],
              ["deepseek-v4-pro", "DeepSeek V4 Pro（质量更好）"],
            ]}
            onChange={(v) => setAi({ deepseekModel: v })}
          />
        ) : (
          <Select
            label={tr("模型")}
            value={ai.model}
            options={[
              ["claude-opus-4-8", "Claude Opus 4.8（默认）"],
              ["claude-opus-5-5", "Claude Opus 5.5（最新，比 4.8 便宜）"],
              ["claude-sonnet-5-5", "Claude Sonnet 5.5（更便宜）"],
              ["claude-sonnet-5", "Claude Sonnet 5"],
              ["claude-haiku-4-5", "Claude Haiku 4.5（最便宜、最快）"],
            ]}
            onChange={(v) => setAi({ model: v })}
          />
        )}
        <Select
          label={tr("每次生成")}
          value={ai.count}
          options={[
            [3, "3 句"],
            [5, "5 句"],
            [8, "8 句"],
          ]}
          onChange={(v) => setAi({ count: v })}
        />
        <Toggle label={tr("查词后自动生成")} hint={tr("关闭时点「生成双语例句」才调用；生成过的词直接显示缓存")} checked={ai.auto} onChange={(v) => setAi({ auto: v })} />
      </div>
      {message && (
        <p className={styles.hint} role={message.error ? "alert" : "status"} data-error={message.error || undefined}>
          {message.text}
        </p>
      )}
      <p className={styles.hint}>{tr("发给 AI 服务的只有：单词、首部词典的简释、COCA 搭配词。不发送语境句和生词本。")}</p>
      <AiUsageStats />
    </section>
  );
}

/** 关于 › 检查更新 */
function UpdateCheck() {
  const [state, setState] = useState<{ kind: "idle" | "busy" } | { kind: "done"; info: UpdateInfo } | { kind: "error"; message: string }>({ kind: "idle" });
  const run = async () => {
    setState({ kind: "busy" });
    try {
      const info = await checkUpdate(false);
      if (info) setState({ kind: "done", info });
      else setState({ kind: "idle" });
    } catch (e) {
      setState({ kind: "error", message: String(e) });
    }
  };
  return (
    <div className={styles.row}>
      <span>
        {tr("检查更新")}
        <small role="status">
          {state.kind === "busy"
            ? tr("正在检查…")
            : state.kind === "error"
              ? state.message
              : state.kind === "done"
                ? state.info.newer
                  ? tr("有新版本 {v}", { v: state.info.latest ?? "" })
                  : state.info.latest
                    ? tr("已是最新版本")
                    : tr("还没有公开发布的版本")
                : tr("从 GitHub 发布页读取最新版本号，不上传任何数据")}
        </small>
      </span>
      {state.kind === "done" && state.info.newer && state.info.url ? (
        <button type="button" className={styles.ghost} onClick={() => state.info.url && void openUrl(state.info.url)}>
          {tr("去下载 ↗")}
        </button>
      ) : (
        <button type="button" className={styles.ghost} disabled={state.kind === "busy"} onClick={() => void run()}>
          {tr("检查更新")}
        </button>
      )}
    </div>
  );
}

/** token 数简写：12.3k、1.2M */
function tokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1000) return `${(n / 1000).toFixed(1)}k`;
  return String(n);
}

function usd(n: number): string {
  if (n === 0) return "$0";
  return n < 0.01 ? `$${n.toFixed(4)}` : `$${n.toFixed(2)}`;
}

/** 设置 › AI 例句 › 用量与花费：按每次调用返回的 token 数和官方单价估算 */
function AiUsageStats() {
  const [data, setData] = useState<UsageSummary | null>(null);
  const [confirm, setConfirm] = useState(false);
  const load = () => {
    const d = new Date();
    const monthStart = new Date(d.getFullYear(), d.getMonth(), 1).getTime();
    aiUsage(monthStart).then(setData, () => setData(null));
  };
  useEffect(load, []);
  if (!data) return null;
  const line = (u: ModelUsage) =>
    tr("{calls} 次 · 输入 {input} · 输出 {output} tokens · 约 {cost}", {
      calls: u.calls,
      input: tokens(u.input + u.cached),
      output: tokens(u.output),
      cost: usd(u.costUsd),
    });
  return (
    <div className={styles.usage}>
      <div className={styles.usageHead}>
        <strong>{tr("用量与花费（估算）")}</strong>
        {data.total.calls > 0 &&
          (confirm ? (
            <span className={styles.inline}>
              <button type="button" className={styles.ghost} onClick={() => void aiUsageClear().then(() => (setConfirm(false), load()))}>
                {tr("确认清零")}
              </button>
              <button type="button" className={styles.ghost} onClick={() => setConfirm(false)}>
                {tr("取消")}
              </button>
            </span>
          ) : (
            <button type="button" className={styles.ghost} onClick={() => setConfirm(true)}>
              {tr("清零")}
            </button>
          ))}
      </div>
      {data.total.calls === 0 ? (
        <p className={styles.hint}>{tr("还没有调用过 AI。每次生成例句后，这里会记下用了多少 token、大约花了多少钱。")}</p>
      ) : (
        <>
          <dl className={styles.usageList}>
            <dt>{tr("本月")}</dt>
            <dd>{line(data.month)}</dd>
            <dt>{tr("累计")}</dt>
            <dd>
              {line(data.total)}
              {data.since && <small>{tr("（自 {date} 起）", { date: new Date(data.since).toLocaleDateString() })}</small>}
            </dd>
            {data.models.map((m) => (
              <Fragment key={m.model}>
                <dt>{m.model}</dt>
                <dd>{line(m)}</dd>
              </Fragment>
            ))}
          </dl>
          <p className={styles.hint}>
            {tr("按每次调用返回的 token 数和官方单价估算（2026 年 10 月价格；DeepSeek 按调用时段区分高峰与半价时段），实际以服务商账单为准。只统计例句生成，缓存命中的词不再计费。")}
          </p>
        </>
      )}
    </div>
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
  const [current, setCurrent] = useState<string>("general");
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
        <section id="set-general" className={styles.section} aria-labelledby="h-general">
          <h2 id="h-general">{tr("常规")}</h2>
          <div className={styles.grid2}>
            <Select
              label={tr("点击关闭按钮时")}
              value={settings.general?.closeToTray ? "tray" : "quit"}
              options={[
                ["quit", "退出 Lens"],
                ["tray", "隐藏到托盘（后台继续运行）"],
              ]}
              onChange={(v) => void updateNow({ general: { closeToTray: v === "tray" } })}
            />
          </div>
          <div className={styles.grid2}>
            <Toggle
              label={tr("启动时检查新版本")}
              hint={tr("每天最多一次，只读 GitHub 发布页的版本号；有新版本时提示")}
              checked={settings.general?.checkUpdates ?? true}
              onChange={(v) => update({ general: { checkUpdates: v } })}
            />
          </div>
          <p className={styles.hint}>
            {tr("隐藏到托盘：点 × 后 Lens 留在后台，在任何程序里划词按快捷键或 Ctrl+C+C 都能随时查词。点任务栏右下角的托盘图标重新打开窗口，右键托盘图标选「退出」才真正关闭。")}
          </p>
        </section>

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
          <Toggle
            label="运行词典自带的脚本"
            hint="有的 mdx 词典靠脚本实现折叠展开、点喇叭发音等交互。脚本在隔离的沙盒里运行，碰不到 Lens 和你的文件"
            checked={a.dictScripts ?? true}
            onChange={(v) => setA({ dictScripts: v })}
          />
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
            <Toggle
              label={tr("词典名称条")}
              hint={tr("查词结果上方一排词典名称，点一下跳到那本词典")}
              checked={l.showJumpBar}
              onChange={(v) => setL({ showJumpBar: v })}
            />
            <Toggle
              label={tr("当前语境")}
              hint={tr("有语境句时（划词、窗口内查词，或自己粘贴一句话），结果上方说明这个词在句中是哪个意思、依据是什么")}
              checked={l.showContextSense ?? true}
              onChange={(v) => setL({ showContextSense: v })}
            />
            <Toggle
              label={tr("词汇网络")}
              hint={tr("查词结果里把派生词、同根词、易混词、近义词画成一张网，点任何一个词就查它（离线）")}
              checked={l.showWordFamily ?? true}
              onChange={(v) => setL({ showWordFamily: v })}
            />
            {(l.showWordFamily ?? true) && (
              <Select
                label="词汇网络的显示方式"
                value={l.wordFamilyView ?? "graph"}
                options={[
                  ["graph", "放射图"],
                  ["table", "表格（词性 + 中文释义）"],
                ]}
                onChange={(v) => setL({ wordFamilyView: v })}
              />
            )}
            <Toggle
              label={tr("义项图谱（实验）")}
              hint={tr("查词结果上方把几部词典的义项对齐，显示共识与差异")}
              checked={l.showSenseAtlas}
              onChange={(v) => setL({ showSenseAtlas: v })}
            />
            <Toggle label={tr("启动时回到简洁单栏")} hint={tr("不管上次用的是哪种布局")} checked={l.startCompact} onChange={(v) => setL({ startCompact: v })} />
          </div>
          <fieldset className={styles.fieldset}>
            <legend>{tr("卡片墙")}</legend>
            <p className={styles.hint}>{tr("各词典卡片按自身长短铺满整个界面：第一排上沿对齐，同一列上下紧挨，列底参差。查词页工具栏的卡片墙按钮或 Ctrl+Shift+W 一键开关。")}</p>
            <div className={styles.grid2}>
              <Toggle label={tr("使用卡片墙")} checked={l.cardWall} onChange={(v) => setL({ cardWall: v })} />
              <Select
                label={tr("每列宽度")}
                value={l.wallColumn}
                options={[
                  [300, "窄（300 px，列多）"],
                  [380, "适中（380 px）"],
                  [460, "宽（460 px，列少）"],
                ]}
                onChange={(v) => setL({ wallColumn: v })}
              />
              <Select
                label={tr("卡片最大高度")}
                value={l.wallMaxHeight}
                options={[
                  [0, "不限（显示完整词条）"],
                  [480, "480 px，超出在卡片内滚动"],
                  [640, "640 px，超出在卡片内滚动"],
                  [800, "800 px，超出在卡片内滚动"],
                ]}
                onChange={(v) => setL({ wallMaxHeight: v })}
              />
            </div>
          </fieldset>
          <PanelSettings />
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

        <ProfileSettings />

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
            <Toggle
              label={tr("vocabulary.com 真实例句")}
              hint={tr("右侧栏和 vocabulary.com 词典卡片里的例句与发音。来自 vocabulary.com 网站的公开接口（非官方），可能随时失效；关闭后不再联网获取")}
              checked={settings.online.vocabExamples ?? true}
              onChange={(v) => update({ online: { vocabExamples: v } })}
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

        <AiSettings />

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
              options={exportStyleChoices(customs)}
              onChange={(v) => setE({ style: v as ExportStyle })}
            />
            <Select
              label={tr("纸张")}
              value={ex.paper === "letter" ? "a4" : ex.paper}
              options={PAPER_CHOICES}
              onChange={(v) => setE({ paper: v as Paper })}
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

        <section id="set-study" className={styles.section} aria-labelledby="h-study">
          <h2 id="h-study">{tr("全书预习包")}</h2>
          <p className={styles.hint}>{tr("阅读 › 导出 › 全书预习包。读前导读要联网查维基百科，只发送书名和作者。")}</p>
          <Toggle
            label={tr("读前导读里放情节梗概和人物介绍")}
            hint={tr("会剧透结局。关掉时只放简介、背景、主题和评价")}
            checked={settings.study?.spoilers ?? false}
            onChange={(v) => update({ study: { spoilers: v } })}
          />
          <div className={styles.grid2}>
            <Select
              label={tr("读前导读的语言")}
              value={settings.study?.guideLang ?? "zh"}
              options={[
                ["zh", "中文优先（没有中文条目时用英文）"],
                ["en", "英文"],
              ]}
              onChange={(v) => update({ study: { guideLang: v as "zh" | "en" } })}
            />
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
          <p className={styles.hint}>{tr("下面这些只在 Lens 窗口里有效。和别的软件习惯冲突时，点击组合键后按下新的即可更换。")}</p>
          <AppKeys />
          <h3 className={styles.sub}>{tr("固定快捷键")}</h3>
          <table className={styles.keys}>
            <caption className="sr-only">{tr("应用内快捷键")}</caption>
            <tbody>
              {SHORTCUTS.map(([k, what]) => (
                <tr key={`${k}·${what}`}>
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
          <div className={styles.grid2}>
            <Select
              label={tr("划词时加入生词本")}
              value={cap.autoSave ? "auto" : "manual"}
              options={[
                ["manual", "手动（点收藏或快捷键）"],
                ["auto", "自动（划词查到就收藏）"],
              ]}
              onChange={(v) => update({ capture: { autoSave: v === "auto" } })}
            />
            <Select
              label={tr("主窗口查词时加入生词本")}
              value={settings.wordbook?.autoSaveLookup ? "auto" : "manual"}
              options={[
                ["manual", "手动（点 ☆ 或快捷键）"],
                ["auto", "自动（按 Enter 查到就收藏）"],
              ]}
              onChange={(v) => update({ wordbook: { autoSaveLookup: v === "auto" } })}
            />
          </div>
          <p className={styles.hint}>{tr("自动：查到释义后收进生词本（划词时连同当时的语境句）；已在生词本里的词只追加语境。主窗口里只有确定的查询才算（按 Enter、点候选、点链接、窗口内划词），打字过程中的联想不算。")}</p>
          <Toggle
            label={tr("查不到的词也能收藏")}
            hint={tr("词典里没有的词组（如 first year）也能收进生词本，连同语境句；释义留空，到生词本里自己写")}
            checked={settings.wordbook?.saveUnmatched ?? true}
            onChange={(v) => update({ wordbook: { saveUnmatched: v } })}
          />
          <div className={styles.grid2}>
            <Select
              label={tr("窗口内查词")}
              value={cap.inAppTrigger ?? "dblclick"}
              options={[
                ["dblclick", "双击查词（推荐）"],
                ["click", "单击查词"],
                ["drag", "划词查词"],
                ["off", "关闭"],
              ]}
              onChange={(v) => update({ capture: { inAppTrigger: v } })}
            />
          </div>
          <p className={styles.hint}>
            {tr("在 Lens 里的释义、真实例句、AI 例句、COCA 搭配和生词本语境句中查词：直接跳到查词页查这个词（在查词小窗里则原地换词），并记下所在句子，收藏时一并保存。双击：最不影响正常操作；单击：点一下就查，像 Kindle；划词：拖选一个词或短语后松开即查（也可双击选词）。按钮、链接、输入框不受影响。")}
          </p>
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
            词镜 Lens {paths ? `v${paths.version}` : ""}{paths?.portable ? ` · ${tr("便携版")}` : ""}
            <br />
            {tr("离线多词典查词、划词取句、复制与导出。软件不附带任何词典；释义内容版权归各词典所有，仅供个人学习使用。")}
          </p>
          <p className={styles.hint}>
            {tr("词性、近义词、派生关系用到了内置的 WordNet 3.1（Copyright 2011 Princeton University，按 WordNet 许可分发）。")}
          </p>
          <p className={styles.hint}>
            {tr("文章生词扫描的内置词频表来自 wordfreq 3.1（Robyn Speer），按 CC BY-SA 4.0 许可分发。")}
          </p>
          <UpdateCheck />
        </section>
      </div>
    </div>
  );
}
