// 与 Rust 端序列化结构一一对应（serde rename_all = camelCase）。

export type EntryBody = { type: "html"; content: string } | { type: "text"; content: string };

export interface RawEntry {
  headword: string;
  body: EntryBody;
}

export interface DictBrief {
  id: string;
  name: string;
}

export interface LookupHandle {
  requestId: string;
  query: string;
  normalized: string;
  dicts: DictBrief[];
}

export type LookupStatus = "hit" | "miss" | "error";

export interface LookupResultEvent {
  requestId: string;
  dictId: string;
  status: LookupStatus;
  entries: RawEntry[];
  error: string | null;
  ms: number;
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
