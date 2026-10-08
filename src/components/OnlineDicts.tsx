// 词典库里的在线词典（SPEC 13、画板 03）：添加面板、每部词典的查询时机 / Key / 网页模板。

import { openUrl } from "@tauri-apps/plugin-opener";
import { useEffect, useState } from "react";
import { addOnlineDict, listOnlineProviders, setOnlineKey, updateOnlineDict } from "../ipc/online";
import type { DictInfo, OnlineOptions, OnlineTrigger, ProviderInfo } from "../ipc/types";
import { useDicts } from "../stores/dicts";
import styles from "./OnlineDicts.module.css";

const TRIGGERS: Array<[OnlineTrigger | "", string]> = [
  ["", "跟随全局设置"],
  ["always", "每次都查"],
  ["localMiss", "本地词典没有结果时才查"],
  ["manual", "手动点击才查"],
];

const EMPTY_OPTIONS: OnlineOptions = { trigger: null, urlTemplate: null, endpoint: null };

export function onlineMeta(d: DictInfo): string {
  if (d.mode === "web") return "网页模式";
  if (d.keyKind && d.keyKind !== "none") return d.hasKey ? "API · 已填 Key" : "API · 未填 Key";
  return "API · 免 Key";
}

export function AddOnlinePanel({ onDone }: { onDone: () => void }) {
  const dicts = useDicts((s) => s.dicts);
  const refresh = useDicts((s) => s.refresh);
  const [providers, setProviders] = useState<ProviderInfo[]>([]);
  const [webName, setWebName] = useState("");
  const [template, setTemplate] = useState("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    listOnlineProviders().then(setProviders, (e) => setError(String(e)));
  }, []);

  const added = new Set(dicts.map((d) => d.provider).filter(Boolean));
  const add = async (provider: string, name?: string, urlTemplate?: string) => {
    setError(null);
    try {
      await addOnlineDict(provider, name, urlTemplate);
      await refresh();
      onDone();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <section className={styles.panel} aria-labelledby="add-online-title">
      <h3 id="add-online-title">添加在线词典</h3>
      <ul className={styles.providers}>
        {providers
          .filter((p) => p.mode === "api")
          .map((p) => (
            <li key={p.id}>
              <span className={styles.pInfo}>
                <strong>{p.name}</strong>
                <small>{p.description}</small>
              </span>
              <button type="button" className={styles.btn} disabled={added.has(p.id)} onClick={() => void add(p.id)}>
                {added.has(p.id) ? "已添加" : "添加"}
              </button>
            </li>
          ))}
      </ul>
      <form
        className={styles.web}
        onSubmit={(e) => {
          e.preventDefault();
          void add("web", webName, template);
        }}
      >
        <strong>网页词典</strong>
        <small>{providers.find((p) => p.id === "web")?.description}</small>
        <div className={styles.webRow}>
          <label>
            <span>名称</span>
            <input value={webName} onChange={(e) => setWebName(e.target.value)} placeholder="Cambridge" />
          </label>
          <label className={styles.grow}>
            <span>网址模板</span>
            <input
              value={template}
              onChange={(e) => setTemplate(e.target.value)}
              placeholder="https://dictionary.cambridge.org/dictionary/english/{word}"
              spellCheck={false}
            />
          </label>
          <button type="submit" className={styles.btn} disabled={!template.trim()}>
            添加
          </button>
        </div>
      </form>
      {error && (
        <p className={styles.error} role="alert">
          {error}
        </p>
      )}
    </section>
  );
}

export function OnlineSettings({ dict }: { dict: DictInfo }) {
  const refresh = useDicts((s) => s.refresh);
  const opts = dict.options ?? EMPTY_OPTIONS;
  const [template, setTemplate] = useState(opts.urlTemplate ?? "");
  const [appId, setAppId] = useState("");
  const [key, setKey] = useState("");
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [keyUrl, setKeyUrl] = useState<string | null>(null);

  useEffect(() => {
    listOnlineProviders().then((ps) => setKeyUrl(ps.find((p) => p.id === dict.provider)?.keyUrl ?? null), () => undefined);
  }, [dict.provider]);

  const save = async (patch: Partial<OnlineOptions>) => {
    setMsg(null);
    try {
      await updateOnlineDict(dict.id, { ...opts, ...patch });
      await refresh();
      setMsg({ ok: true, text: "已保存" });
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    }
  };

  const saveKey = async (value: string) => {
    setMsg(null);
    try {
      const has = await setOnlineKey(dict.id, value, appId);
      setKey("");
      setAppId("");
      await refresh();
      setMsg({ ok: true, text: has ? "Key 已保存到 Windows 凭据管理器" : "Key 已清除" });
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    }
  };

  return (
    <div className={styles.settings}>
      <label className={styles.field}>
        <span>查询时机</span>
        <select value={opts.trigger ?? ""} onChange={(e) => void save({ trigger: (e.target.value || null) as OnlineTrigger | null })}>
          {TRIGGERS.map(([v, name]) => (
            <option key={v} value={v}>
              {name}
            </option>
          ))}
        </select>
      </label>

      {dict.provider === "oxford" && (
        <label className={styles.field}>
          <span>接口</span>
          <select value={opts.endpoint ?? "production"} onChange={(e) => void save({ endpoint: e.target.value as "production" | "sandbox" })}>
            <option value="production">正式（付费套餐）</option>
            <option value="sandbox">试用沙盒（共 500 次，只能查 a 开头的词）</option>
          </select>
        </label>
      )}

      {dict.mode === "web" && (
        <form
          className={styles.field}
          onSubmit={(e) => {
            e.preventDefault();
            void save({ urlTemplate: template.trim() });
          }}
        >
          <span>网址模板</span>
          <span className={styles.inline}>
            <input value={template} onChange={(e) => setTemplate(e.target.value)} spellCheck={false} aria-label="网址模板" />
            <button type="submit" className={styles.btn}>
              保存
            </button>
          </span>
        </form>
      )}

      {dict.keyKind && dict.keyKind !== "none" && (
        <form
          className={styles.field}
          onSubmit={(e) => {
            e.preventDefault();
            void saveKey(key);
          }}
        >
          <span>
            {dict.keyKind === "idAndKey" ? "App ID / App Key" : "API Key"}
            <small>{dict.hasKey ? "已保存（不显示）" : "未填写"} · 存在 Windows 凭据管理器，不写进设置文件</small>
          </span>
          <span className={styles.inline}>
            {dict.keyKind === "idAndKey" && (
              <input value={appId} onChange={(e) => setAppId(e.target.value)} placeholder="App ID" aria-label="App ID" spellCheck={false} autoComplete="off" />
            )}
            <input type="password" value={key} onChange={(e) => setKey(e.target.value)} placeholder={dict.hasKey ? "输入新 Key 以替换" : "粘贴 Key"} aria-label="API Key" autoComplete="off" />
            <button type="submit" className={styles.btn} disabled={!key.trim()}>
              保存
            </button>
            {dict.hasKey && (
              <button type="button" className={styles.ghost} onClick={() => void saveKey("")}>
                清除
              </button>
            )}
          </span>
          {keyUrl && (
            <a
              className={styles.link}
              href={keyUrl}
              onClick={(e) => {
                e.preventDefault();
                void openUrl(keyUrl);
              }}
            >
              去官网申请 Key ↗
            </a>
          )}
        </form>
      )}

      {msg && (
        <p className={msg.ok ? styles.ok : styles.error} role="status">
          {msg.text}
        </p>
      )}
    </div>
  );
}
