// 与 Rust 端序列化结构一一对应（serde rename_all = camelCase）。

export type EntryBody = { type: "html"; content: string } | { type: "text"; content: string };

export interface RawEntry {
  headword: string;
  body: EntryBody;
}

export interface DictBrief {
  id: string;
  name: string;
  /** local：本地词典；api：在线接口；web：网页模式 */
  source: "local" | "api" | "web";
}

export interface LookupHandle {
  requestId: string;
  query: string;
  normalized: string;
  /** 原形和查询词不同时（left → leave）；词典收了 left 时不会再查 leave，界面据此提示 */
  base: string | null;
  dicts: DictBrief[];
}

/** 在线词典另有：manual 等用户点击、web 网页模式（给出地址）、skipped 本地已命中而未查 */
export type LookupStatus = "hit" | "miss" | "error" | "manual" | "web" | "skipped";

export interface LookupResultEvent {
  requestId: string;
  dictId: string;
  status: LookupStatus;
  entries: RawEntry[];
  error: string | null;
  ms: number;
  /** 在线结果来自缓存时的获取时间（毫秒） */
  cachedAt: number | null;
  /** 缓存已过期：联网失败后的回退 */
  stale: boolean;
  /** 网页模式的原页面地址 */
  webUrl: string | null;
}

export interface LookupDoneEvent {
  requestId: string;
  suggestions: string[];
}

export interface DictInfo {
  id: string;
  name: string;
  kind: "mdx" | "stardict" | "json" | "online";
  category: string | null;
  entryCount: number | null;
  path: string | null;
  indexState: "pending" | "building" | "ready" | "error";
  loaded: boolean;
  /** 本地词典的文件不在记录的位置 */
  missing: boolean;
  /** 在线词典 */
  provider: string | null;
  mode: OnlineMode | null;
  keyKind: KeyKind | null;
  hasKey: boolean;
  options: OnlineOptions | null;
  /** 启用这部词典的场景配置组 id */
  profiles: string[];
}

export type OnlineMode = "api" | "web";
export type KeyKind = "none" | "key" | "idAndKey";
export type OnlineTrigger = "always" | "localMiss" | "manual";

export interface OnlineOptions {
  /** null = 跟随全局设置 */
  trigger: OnlineTrigger | null;
  urlTemplate: string | null;
  endpoint: "production" | "sandbox" | null;
}

export interface ProviderInfo {
  id: string;
  name: string;
  mode: OnlineMode;
  keyKind: KeyKind;
  category: string;
  description: string;
  keyUrl: string | null;
}

export interface IndexProgress {
  path: string;
  index: number;
  total: number;
  phase: "start" | "done" | "error";
  dictId: string | null;
  name: string | null;
  error: string | null;
}

export interface ProfileInfo {
  id: string;
  name: string;
  dictCount: number;
}
