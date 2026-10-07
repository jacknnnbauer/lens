# 词镜 Lens · 开发说明（v0.1）

> 本文是实现用的规格说明，配合设计画布阅读：
> 设计稿：https://claude.ai/code/artifact/0cf57411-c890-42c7-b38a-63a49b6a897f
> （画板 00 总览；01 主窗口；02 迷你弹窗；03 词典库；04 生词本；05 简洁单栏；06 设置；07/08 单词导出；09 主题库；10/11 批量导出；12 沉浸模式）
>
> 平台：**仅 Windows 10 1809+ / Windows 11**（第一版）。语言：界面中文，词典以英语为主。

---

## 1. 产品一句话

本地优先、键盘优先的桌面查词工具：划词按快捷键 → 弹窗显示多本词典的释义，并自动捕获单词所在的句子；结果可复制、可导出、可收进生词本复习。

---

## 2. 范围与里程碑

按里程碑推进，每个里程碑结束时都能运行、能演示。**建议 v0.1 = M0–M5**，M6–M8 为 v0.2。

| 里程碑 | 内容 | 验收标准 |
|---|---|---|
| M0 脚手架 | Tauri 2 + React + TS + Vite；SQLite 迁移；主题变量；日志 | `pnpm tauri dev` 启动出空主窗口；切换 3 套主题生效 |
| M1 词典引擎 | mdx/mdd、StarDict、JSON 解析；建 FST 索引；前缀/模糊查询；拼写纠错；词形还原 | 导入一部 100MB 级 mdx，冷启动后查词 < 50ms；`cargo test` 覆盖三种格式 |
| M2 主窗口查词 | 搜索框（防抖）、候选下拉、多词典卡片（折叠/展开）、Shadow DOM 渲染、mdd 资源、发音 | 三栏 + 简洁单栏两种布局可切换；全键盘可操作 |
| M3 划词与弹窗 | 全局快捷键、Ctrl+C+C、UIA 取词 + 语境句、迷你弹窗、剪贴板回退 | 在 Edge / 记事本 / Word 中划词弹窗 < 100ms，句子正确 |
| M4 历史与生词本 | 历史记录页、收藏（带语境句）、标签、语境卡 | 收藏后在生词本看到原句与来源，可「回到原处」（浏览器 URL） |
| M5 复制、单词导出、设置、沉浸模式 | 卡片复制 MD/HTML/纯文本；单词导出 PDF/HTML（卡片式/文档式）；设置页；F11 沉浸模式；纸本/冷白/石墨 3 套主题 + 5 强调色 | 粘贴到 Word 保留格式，粘贴到 Obsidian 为合法 Markdown；导出 PDF 文字可搜索 |
| M6 在线词典 | Provider 接口、API 模式、网页模式、缓存、限速 | 断网时显示缓存并标注时间；超时不阻塞本地结果 |
| M7 批量导出 | 历史 / 粘贴词表 / 生词本 → 多个 PDF/HTML；流水线、断点续传 | 2000 词批量导出内存峰值 < 800MB；中途杀进程重启可续 |
| M8 其余主题、主题库、义项图谱、FSRS 复习 | 另外 8 套主题；主题导入导出；义项对齐；间隔复习 | — |

**v0.1 明确不做**：macOS、账号/云同步、内置任何受版权保护的词典、加密 mdx 破解、第三方网页抓取解析器、AI 功能。

---

## 3. 技术栈

### 后端（Rust，`src-tauri/`）

| 用途 | crate |
|---|---|
| 桌面外壳 | `tauri` 2.x、`tauri-plugin-global-shortcut`、`tauri-plugin-dialog`、`tauri-plugin-fs`、`tauri-plugin-single-instance` |
| Windows API | `windows`（UI Automation、键盘钩子、剪贴板、SetThreadExecutionState、Windows.Media.Ocr） |
| 存储 | `rusqlite`（features: `bundled`，含 FTS5）、`refinery` 或手写迁移 |
| 索引 | `fst` |
| 解压 | `flate2`（zlib）、LZO 用 `minilzo-rs` 或 `lzokay-native`；dictzip 按 gzip 随机访问自行实现 |
| 编码 | `encoding_rs`（UTF-16LE、GBK、Big5） |
| XML | `quick-xml`（mdx 头部） |
| 断句 | `icu_segmenter` |
| 纠错 | `symspell` |
| 网络 | `reqwest`（rustls）、`governor`（令牌桶限速） |
| 并发 | `tokio` |
| 其他 | `serde`、`serde_json`、`thiserror`、`anyhow`、`tracing`、`tracing-appender` |

### 前端（`src/`）

React 18+、TypeScript、Vite；`zustand`（UI 状态）、`@tanstack/react-query`（查询缓存/取消/流式合并）、`@tanstack/react-virtual`（长列表）、`@radix-ui/*`（无障碍原语）、`cmdk`（命令面板）、`dompurify`（清洗词典 HTML）、`turndown`（HTML→Markdown）。样式用 CSS 变量 + CSS Modules（或 vanilla-extract），**不用 Tailwind 的硬编码颜色**，所有颜色走主题变量。

---

## 4. 目录结构（建议）

```
lens/
├─ SPEC.md                    # 本文
├─ CLAUDE.md                  # 给 Claude Code 的工作约定
├─ src-tauri/
│  ├─ src/
│  │  ├─ main.rs
│  │  ├─ app_state.rs         # 全局状态：DictRegistry、Db、Settings
│  │  ├─ commands/            # Tauri 命令（IPC 入口），薄封装
│  │  ├─ dict/
│  │  │  ├─ mod.rs            # DictSource trait、DictRegistry
│  │  │  ├─ mdx/              # header / keyblock / recordblock / mdd
│  │  │  ├─ stardict/
│  │  │  ├─ json/
│  │  │  └─ index.rs          # FST 构建与加载
│  │  ├─ query/               # 归一化、词形还原、纠错、意图识别、并行查询
│  │  ├─ capture/             # 全局热键、Ctrl+C+C 钩子、UIA、OCR、断句
│  │  ├─ online/              # Provider trait、各站点、缓存、限速
│  │  ├─ export/              # 单词导出、批量导出流水线
│  │  ├─ clipboard.rs         # 多格式写剪贴板
│  │  ├─ db/                  # 迁移、仓储（history、words、contexts、tags…）
│  │  └─ protocol.rs          # lens:// 自定义协议（mdd 资源）
│  └─ tauri.conf.json
├─ src/
│  ├─ main.tsx
│  ├─ windows/                # main（主窗口）、popup（迷你弹窗）、export-render（隐藏渲染窗口）
│  ├─ routes/                 # lookup、wordbook、history、library、settings
│  ├─ components/             # SearchBox、DictCard、CopyMenu、ContextSentence、…
│  ├─ dict-render/            # Shadow DOM 渲染器、清洗、资源重写
│  ├─ theme/                  # tokens.css、themes.ts
│  ├─ stores/                 # zustand
│  ├─ ipc/                    # 类型化的 invoke / listen 封装
│  └─ export-templates/       # 卡片式、文档式打印模板（PDF 与 HTML 共用）
└─ fixtures/                  # 测试用小词典（自制，勿提交商业词典）
```

---

## 5. 架构

```
┌─────────── 前端（WebView2）───────────┐      ┌──────────── Rust 后端 ────────────┐
│ main 窗口   popup 窗口   export-render │◄────►│ commands ─► query ─► DictRegistry │
│   React       React       隐藏窗口     │ IPC  │              │          ├ mdx      │
│                                        │事件流│              │          ├ stardict │
└────────────────────────────────────────┘      │              │          └ json     │
                                                │              └─► online (M6)       │
                                                │ capture ─► 热键/钩子/UIA/OCR        │
                                                │ export  ─► 流水线 (M7)              │
                                                │ db      ─► SQLite (WAL)             │
                                                └─────────────────────────────────────┘
```

**原则**
- 词典解析、索引、数据库、取词、导出调度全部在 Rust；前端只负责展示和交互。
- 迷你弹窗是**常驻隐藏窗口**（启动时创建，用时 show/hide），不要每次新建，否则达不到 100ms。
- 查询结果**按词典逐个推送**（Tauri event），前端先显示词头和骨架卡片，到一个填一个。
- 所有长任务（建索引、批量导出）在后台 tokio 任务里跑，进度通过事件发出，**节流 200ms**。

### 5.1 IPC 命令（Tauri commands）

```ts
// 查询
lookup(query: string, opts: { profileId?: string; source: 'main'|'popup' }): Promise<LookupHandle>
//   立即返回 { requestId, normalized, lemma, intent }；结果通过事件 `lookup:result` 逐条推送
cancel_lookup(requestId: string): void
suggest(prefix: string, limit: number): Promise<Suggestion[]>   // 前缀 + 纠错候选

// 词典库
list_dicts(): Promise<DictMeta[]>
import_dicts(paths: string[]): Promise<ImportJob>               // 进度事件 `dict:index-progress`
update_dict(id: string, patch: Partial<DictSettings>): Promise<void>
reorder_dicts(profileId: string, ids: string[]): Promise<void>
remove_dict(id: string): Promise<void>

// 历史 / 生词本
list_history(range: DateRange, page: Page): Promise<HistoryItem[]>
save_word(input: { headword: string; context?: ContextInput; tags?: string[] }): Promise<WordId>
list_words(filter: WordFilter, page: Page): Promise<Word[]>
add_context(wordId: WordId, ctx: ContextInput): Promise<void>

// 复制 / 导出
copy_entry(entryKey: EntryKey, format: 'md'|'html'|'text', withSource: boolean): Promise<void>
export_word(req: ExportRequest): Promise<string /* 输出路径 */>
start_batch_export(req: BatchExportRequest): Promise<BatchId>   // 事件 `batch:progress`
pause_batch / resume_batch / cancel_batch(batchId)
list_unfinished_batches(): Promise<BatchSummary[]>

// 设置
get_settings(): Promise<Settings>
set_settings(patch: Partial<Settings>): Promise<void>
```

### 5.2 事件

| 事件 | 载荷 |
|---|---|
| `lookup:result` | `{ requestId, dictId, status: 'hit'|'miss'|'error', html?, plain?, ms }` |
| `lookup:done` | `{ requestId }` |
| `capture:triggered` | `{ text, sentence?, source: CaptureSource, rect }` → popup 窗口监听 |
| `dict:index-progress` | `{ dictId, phase, done, total }` |
| `batch:progress` | `{ batchId, done, total, stage counts, mem, current[] }`（节流 200ms） |

---

## 6. 数据库（SQLite，WAL 模式）

```sql
-- 词典
CREATE TABLE dict (
  id            TEXT PRIMARY KEY,           -- uuid
  name          TEXT NOT NULL,
  kind          TEXT NOT NULL,              -- 'mdx' | 'stardict' | 'json' | 'online'
  path          TEXT,                       -- 本地文件路径（online 为空）
  provider      TEXT,                       -- online 时的 provider id
  category      TEXT,                       -- 英英/英汉/专业/词源…
  entry_count   INTEGER,
  index_path    TEXT,                       -- FST 文件路径（%APPDATA%/Lens/index/<id>.fst）
  index_state   TEXT NOT NULL DEFAULT 'pending', -- pending|building|ready|error
  css_override  INTEGER NOT NULL DEFAULT 0,
  created_at    INTEGER NOT NULL
);

-- 场景配置组（阅读/翻译/编程）
CREATE TABLE profile (id TEXT PRIMARY KEY, name TEXT NOT NULL, sort INTEGER NOT NULL);
CREATE TABLE profile_dict (
  profile_id TEXT NOT NULL REFERENCES profile(id) ON DELETE CASCADE,
  dict_id    TEXT NOT NULL REFERENCES dict(id)    ON DELETE CASCADE,
  enabled    INTEGER NOT NULL DEFAULT 1,
  sort       INTEGER NOT NULL,
  in_popup   INTEGER NOT NULL DEFAULT 1,
  expanded   INTEGER NOT NULL DEFAULT 0,
  query_kinds TEXT NOT NULL DEFAULT 'word,phrase', -- word,phrase,zh,wildcard
  PRIMARY KEY (profile_id, dict_id)
);

-- 查询历史（每次查询一行）
CREATE TABLE history (
  id         INTEGER PRIMARY KEY,
  query      TEXT NOT NULL,                 -- 用户输入原文
  lemma      TEXT NOT NULL,                 -- 归一化后的原形
  source     TEXT NOT NULL,                 -- main|popup|batch
  context_id INTEGER REFERENCES context(id),
  created_at INTEGER NOT NULL
);
CREATE INDEX history_lemma ON history(lemma);
CREATE INDEX history_time  ON history(created_at);

-- 语境句（每次划词捕获一条）
CREATE TABLE context (
  id          INTEGER PRIMARY KEY,
  lemma       TEXT NOT NULL,
  surface     TEXT NOT NULL,                -- 句中原形（如 ran）
  sentence    TEXT NOT NULL,
  hl_start    INTEGER NOT NULL,             -- 单词在句中的 UTF-16 偏移，用于高亮
  hl_end      INTEGER NOT NULL,
  app         TEXT,                         -- 进程名，如 msedge.exe
  window_title TEXT,
  url         TEXT,
  file_path   TEXT,
  page        INTEGER,
  method      TEXT NOT NULL,                -- uia|ocr|clipboard|manual
  created_at  INTEGER NOT NULL
);

-- 生词本
CREATE TABLE word (
  id          INTEGER PRIMARY KEY,
  lemma       TEXT NOT NULL UNIQUE,
  brief       TEXT,                         -- 一句话释义（取首选词典第一义项）
  familiarity INTEGER NOT NULL DEFAULT 0,   -- 0..5
  due_at      INTEGER,                      -- 下次复习（M8 接 FSRS）
  created_at  INTEGER NOT NULL
);
CREATE TABLE word_context (word_id INTEGER REFERENCES word(id) ON DELETE CASCADE,
                           context_id INTEGER REFERENCES context(id) ON DELETE CASCADE,
                           PRIMARY KEY (word_id, context_id));
CREATE TABLE tag (id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE);
CREATE TABLE word_tag (word_id INTEGER REFERENCES word(id) ON DELETE CASCADE,
                       tag_id INTEGER REFERENCES tag(id) ON DELETE CASCADE,
                       PRIMARY KEY (word_id, tag_id));

-- 在线词典缓存（M6）
CREATE TABLE online_cache (
  provider TEXT NOT NULL, lemma TEXT NOT NULL,
  payload  TEXT NOT NULL, fetched_at INTEGER NOT NULL,
  PRIMARY KEY (provider, lemma)
);

-- 批量导出（M7，断点续传）
CREATE TABLE batch (
  id TEXT PRIMARY KEY, request_json TEXT NOT NULL,
  state TEXT NOT NULL,                      -- running|paused|cancelled|done
  out_dir TEXT NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
);
CREATE TABLE batch_item (
  batch_id TEXT NOT NULL REFERENCES batch(id) ON DELETE CASCADE,
  seq INTEGER NOT NULL, lemma TEXT NOT NULL,
  state TEXT NOT NULL,                      -- pending|querying|rendering|done|failed|skipped
  note TEXT, files TEXT,                    -- 失败原因 / 生成的文件名（JSON 数组）
  PRIMARY KEY (batch_id, seq)
);

-- 全文反查（中文查英文）：对每部词典的纯文本释义建 FTS5（可选，导入时勾选）
CREATE VIRTUAL TABLE gloss_fts USING fts5(dict_id UNINDEXED, headword UNINDEXED, text, tokenize='trigram');
```

设置存成 `%APPDATA%/Lens/settings.json`（见第 13 节），不放数据库，方便用户手改和备份。

---

## 7. 词典引擎（M1）

### 7.1 统一接口

```rust
pub trait DictSource: Send + Sync {
    fn meta(&self) -> &DictMeta;
    /// 精确查词条，返回原始 HTML（或纯文本），可能多条（同形异义）
    fn lookup(&self, key: &str) -> Result<Vec<RawEntry>>;
    /// 读取资源（mdd 里的图片/音频/CSS），路径为词典内相对路径
    fn resource(&self, path: &str) -> Result<Option<Vec<u8>>>;
}
pub struct RawEntry { pub headword: String, pub body: EntryBody } // Html(String) | Text(String)
```

`DictRegistry` 持有所有已加载词典（`Arc<dyn DictSource>`）和各自的 FST。查询时按当前配置组的顺序并发查（`tokio::task::spawn_blocking`，并发上限 = CPU 核数），每得到一个结果就发事件。

### 7.2 索引

- 导入时遍历全部词头 → 归一化（NFC、小写、去重音可选）→ 构建 `fst::Map<key → u64>`，值编码为「记录位置」（mdx：record block 序号 + 块内偏移；StarDict：.idx 中的 offset/size）。
- FST 文件存 `%APPDATA%/Lens/index/<dictId>.fst`，启动时 `mmap` 加载，**不把整部词典读进内存**。
- 同一个归一化键对应多条时，值指向一个小的冲突表。
- 前缀查询：`fst` 的 `StartsWith` 自动机；模糊：`Levenshtein` 自动机（距离 ≤ 2，只对长度 ≥ 4 的输入启用）。

### 7.3 mdx / mdd 解析要点

- 文件结构：`header_len(u32 BE)` + 头部 XML（UTF-16LE）+ adler32 → key block info → key blocks → record block info → record blocks。
- 头部属性：`GeneratedByEngineVersion`（1.2 / 2.0，决定整数宽度 4/8 字节）、`Encoding`（UTF-8/UTF-16/GBK…）、`Encrypted`、`StyleSheet`。
- 压缩类型：块头 4 字节，`0`=无压缩，`1`=LZO，`2`=zlib。
- `Encrypted & 2`：key block info 用 ripemd128 派生的密钥做简单异或加密，可以解。`Encrypted & 1`（需要注册码）**不支持**，导入时提示「此词典需要授权文件」。
- 记录正文可能含 `@@@LINK=xxx`（重定向），查询时跟随，最多 5 层。
- `StyleSheet` 替换规则：正文里的 `` `1` `` 等编号替换为头部定义的样式标签。
- mdd 与 mdx 同结构，key 是资源路径（如 `\img\a.png`），统一转成 `/` 分隔、小写比较。
- 词条 HTML 中的 `entry://word` 链接改写为站内查词；`sound://x.mp3` 改写为 `lens://res/<dictId>/x.mp3`。

### 7.4 StarDict

- `.ifo`（元信息）、`.idx`（词头 + offset + size，可能是 `.idx.gz`）、`.dict` 或 `.dict.dz`（dictzip，按 chunk 随机访问 gzip）、可选 `.syn`（同义词表 → 并入 FST）。
- `sametypesequence` 决定条目类型：`m` 纯文本、`h` HTML、`x` XDXF 等。v0.1 支持 `m`、`h`、`g`，其余按纯文本显示。

### 7.5 JSON 词典（自定义格式，供词根词缀等小词典用）

```json
{ "name": "词根词缀", "category": "词源", "version": 1,
  "entries": [ { "headword": "ubiquitous", "html": "<p>ubique 处处 ← ubi 哪里 + -ous</p>", "aliases": ["ubiquity"] } ] }
```

### 7.6 查询管线

1. **意图识别**：含 CJK → 中文反查（FTS）；含 `*` `?` → 通配（FST 正则自动机）；多于 4 个词 → 句子模式（逐词可点）；否则单词/短语。
2. **归一化**：trim、去首尾标点、NFC、小写（保留原输入用于显示）、合并连字符断行。
3. **词形还原**：内置 lemma 表（屈折形 → 原形，HashMap，启动时加载，约 10 万条）；先查原输入，没有再查原形。
4. **纠错**：所有词典都没命中时，用 SymSpell（字典 = 所有词头的并集，导入时增量更新）给出 ≤ 5 个候选。
5. **防抖**：输入框 100ms；新查询发出时取消旧的 `requestId`。

**性能预算**：单部词典精确查 < 5ms；6 部并行首个结果 < 50ms；冷启动到可查词 < 1.5s。

---

## 8. 取词与语境句（M3）

### 8.1 触发

- **全局快捷键**：`tauri-plugin-global-shortcut` 注册 `Ctrl+Alt+Space`（呼出空白查词框），可在设置中修改。
- **Ctrl+C+C**：`SetWindowsHookExW(WH_KEYBOARD_LL)` 低级键盘钩子，检测 400ms 内两次 Ctrl+C。钩子回调里只做计时和投递消息，**不做任何耗时操作**，否则会拖慢全系统输入。
- 默认避开 `Alt+Space`（系统窗口菜单）和 `Ctrl+Space`（输入法切换）。
- 输入法正在组字（IME composition）时不触发。

### 8.2 取选区与周边文本

1. UI Automation：`IUIAutomation::GetFocusedElement` → `TextPattern` → `GetSelection()` 得到选中的 `TextRange`。
2. 复制该 range，`ExpandToEnclosingUnit(TextUnit_Paragraph)`；不支持则退到 `Line`；再不行用 `MoveEndpointByUnit(Character, ±300)`。
3. 得到「周边文本 + 选区在其中的偏移」。
4. 来源：`GetForegroundWindow` → 进程名、窗口标题；浏览器（Edge/Chrome/Firefox）通过 UIA 找地址栏控件读 URL。
5. **回退 A**：UIA 拿不到 → 读剪贴板（Ctrl+C+C 场景用户已经复制了），只有单词、没有句子。
6. **回退 B**（设置里可开）：截取选区附近区域（以光标为中心 800×200），`Windows.Media.Ocr` 识别，找到包含所选词的那一行及前后行。
7. 整个取词过程设 **150ms 超时**，超时就用已有结果，先把弹窗弹出来。

### 8.3 断句

- `icu_segmenter::SentenceSegmenter` 切出包含选区偏移的那一句。
- 预处理：合并 `-\n` 断词；把单个换行替换成空格（PDF 复制常见）；压缩空白。
- 后处理：如果切点落在常见缩写后（`Mr.` `Dr.` `e.g.` `i.e.` `U.S.` `etc.` `No.`）或小数点中，向后合并。
- 句子超过 400 字符：以单词为中心各取 180 字符，两端加 `…`。
- 记录高亮区间 `hl_start/hl_end`（UTF-16 偏移，前端直接用）。

### 8.4 弹窗定位

- 以选区矩形（UIA `GetBoundingRectangles`，没有就用鼠标位置）为锚点，优先显示在下方，空间不够放上方；左右夹在当前显示器工作区内。
- 多显示器、不同 DPI：用 `MonitorFromPoint` + `GetDpiForMonitor` 换算，Tauri 窗口用物理像素定位。
- 弹窗失焦 300ms 后自动隐藏（钉住状态除外，Ctrl+P）。

---

## 9. 前端

### 9.1 窗口与路由

| 窗口 | 说明 |
|---|---|
| `main` | 主窗口；路由：`/lookup`、`/wordbook`、`/history`、`/library`、`/settings` |
| `popup` | 迷你弹窗，无边框、置顶、不在任务栏显示，常驻隐藏 |
| `export-render` | 隐藏窗口，只用于导出时渲染打印模板（M5 起） |

### 9.2 布局

- `layout = 'three-column' | 'compact' | 'two-column'`，各区域开关（左侧导航、义项图谱、上下文栏、状态栏、跳转条）独立存于设置。
- **沉浸模式**（F11 / Esc）：隐藏标题栏（Tauri `decorations: false`，自绘拖拽区）、状态栏、侧栏、跳转条和复制按钮；正文宽度 600/720/900；鼠标在顶部 48px 内停留 300ms 浮现工具栏，离开 1.5s 隐藏；J/K 上下翻词典；可选单独主题；可选全屏。

### 9.3 词典卡片 `DictCard`

- 头部：词典名、离线/在线标记、复制分裂按钮（主按钮 = 默认格式，下拉 = MD / HTML / 纯文本 / 设为默认）。
- 正文：**Shadow DOM** 挂载，注入该词典自带 CSS + 主题覆盖层（可选）。
- 清洗：DOMPurify，禁止 `<script>`、事件属性、`javascript:`；外链默认用系统浏览器打开。
- 资源：`<img src>`、`<link href>`、`sound://` 统一重写为 `lens://res/<dictId>/<path>`，由 Rust 自定义协议从 mdd 读取（带 LRU 缓存，上限 64MB）。
- 折叠状态显示一行摘要（取正文纯文本前 120 字）。
- 骨架占位：未返回的词典显示固定高度骨架，避免布局跳动。

### 9.4 键盘

| 键 | 作用 |
|---|---|
| Ctrl+K | 聚焦搜索框 |
| ↑ ↓ | 候选 / 义项移动 |
| Tab / Shift+Tab | 下一本 / 上一本词典 |
| Space | 展开 / 折叠当前词典 |
| Enter | 弹窗中：展开到主窗口 |
| Ctrl+S | 收藏（带语境句） |
| Ctrl+Shift+C | 复制当前词典（默认格式） |
| Ctrl+Shift+A | 合并复制所有展开的词典 |
| Ctrl+E | 导出当前词 |
| Ctrl+1/2/3 | 切换场景配置组 |
| Alt+← / Alt+→ | 弹窗中：语境句向前/向后多带一句 |
| F11 / Esc | 进入 / 退出沉浸模式 |
| Ctrl+, | 设置 |

所有可交互元素必须是真实的 `<button>` / `<a>` / `<input>`，有可见焦点环。

### 9.5 主题

所有颜色只用以下变量。主题 = 一组变量值；强调色 = 覆盖 `--accent`（只对纸本/冷白/石墨生效）。高亮色 `--hl: color-mix(in srgb, var(--accent) 14%, transparent)`。

| 主题 id | 名称 | bg | panel | raised | line | text | sub | faint | accent | accent-ink | warn |
|---|---|---|---|---|---|---|---|---|---|---|---|
| paper | 纸本 | #F7F6F3 | #FFFFFF | #F0EFEB | #E2E0DA | #1A1B1E | #4E5258 | #666A71 | #2F5FD0 | #FFFFFF | #A64F0C |
| light | 冷白 | #F3F4F6 | #FFFFFF | #ECEEF1 | #DDE0E5 | #15171B | #4A4F57 | #626872 | #2F5FD0 | #FFFFFF | #A64F0C |
| dark | 石墨 | #141518 | #1B1D21 | #23262B | #2C3036 | #ECEAE4 | #A3A7AF | #8A8F98 | #8AB4F8 | #0E1A2E | #F2A65A |
| xuan | 宣纸 | #F1EADB | #FAF6EC | #E9E0CC | #D9CDB4 | #23201B | #4F483D | #6B6253 | #A93226 | #FFFFFF | #7A5A12 |
| sepia | 护眼棕 | #F3E9D2 | #F8F0DC | #EADDC0 | #D8C7A3 | #3B2F22 | #5A4A37 | #6E5C46 | #8A4B1C | #FFFFFF | #4F6419 |
| frost | 霜蓝 | #EEF2F6 | #FFFFFF | #E3E9F0 | #D3DCE6 | #1B2430 | #44505F | #5D6979 | #1F6E92 | #FFFFFF | #A6501A |
| eink | 墨水屏 | #FFFFFF | #FFFFFF | #F0F0F0 | #9E9E9E | #000000 | #2B2B2B | #4D4D4D | #000000 | #FFFFFF | #000000 |
| midnight | 午夜蓝 | #0E1424 | #151D31 | #1C2640 | #27324D | #E6EAF2 | #A6B0C3 | #8893A8 | #E3B663 | #1A1405 | #F08A7E |
| moss | 苔原 | #121A16 | #18231D | #1F2D25 | #2A3A31 | #E4EBE2 | #A5B3A7 | #87968A | #9FD49A | #0F1A10 | #E7B062 |
| dusk | 暮紫 | #18151F | #201C29 | #2A2535 | #352F43 | #ECE7F3 | #ADA5BD | #8F87A0 | #F0A0B8 | #2A0F18 | #F2C46B |
| contrast | 高对比 | #000000 | #0A0A0A | #1A1A1A | #7A7A7A | #FFFFFF | #E6E6E6 | #CCCCCC | #FFD400 | #000000 | #5CE1E6 |

强调色预设（亮 / 暗两套值）：

| 名称 | 亮色主题 | 石墨 |
|---|---|---|
| 靛蓝 blue | #2F5FD0 | #8AB4F8 |
| 青绿 teal | #0F7A6E | #5FD3C4 |
| 琥珀 amber | #9A5B00 | #F2B661 |
| 紫罗兰 violet | #6B4FD0 | #B9A3F8 |
| 玫红 rose | #B4325C | #F28FAD |

主题顺序：亮色在前（纸本、冷白、宣纸、护眼棕、霜蓝、墨水屏），暗色在后（石墨、午夜蓝、苔原、暮紫、高对比）。v0.1 只上纸本、冷白、石墨，其余在 M8。

**字体**：界面 `"IBM Plex Sans", "Microsoft YaHei UI", "Noto Sans SC", sans-serif`；释义 `"Source Serif 4", "Noto Serif SC", "SimSun", serif`；音标/快捷键 `"JetBrains Mono", Consolas, monospace`。字体文件随安装包分发（均为 OFL 许可），不依赖网络。

---

## 10. 复制（M5）

| 格式 | 实现 |
|---|---|
| Markdown | DOMPurify 清洗 → Turndown（自定义规则：义项编号保留为有序列表、例句为斜体子列表、音标包成行内代码、图片转链接、去掉空 span 和样式）→ 可选末尾 `> 来源：<词典> · <词条>` |
| HTML | 清洗后把词典 CSS **内联到元素 style**（用浏览器 `getComputedStyle` 遍历，只保留字体、颜色、粗斜体、缩进等白名单属性）→ Rust 侧同时写入 `CF_HTML`（带 `StartFragment/EndFragment` 头）和 `CF_UNICODETEXT` |
| 纯文本 | 按块级元素换行，义项缩进两个空格 |

语境句的 Markdown：`> …made tracking **ubiquitous** in modern warehouses.` 下一行 `> — [标题](url) · Edge · 2026-10-07`。

---

## 11. 单词导出（M5）

- 两种版式：**卡片式**（默认，和简洁单栏布局一致：顶部单词框 + 词典标签行 + 每本词典一张圆角卡片 + 语境句卡片）和**文档式**（单词标题 + 每本词典一节，分隔线隔开）。模板见画板 08。
- 样式：纸本白底 / 跟随当前主题 / 黑白省墨；纸张 A4 / Letter。
- **PDF**：在 `export-render` 隐藏窗口加载打印模板，调用 WebView2 的 `ICoreWebView2_7::PrintToPdf`（通过 `webview.with_webview` 拿到原生 controller）。字体嵌入、文字可选可搜。卡片加 `break-inside: avoid`，超过一页的卡片才允许拆分。
- **HTML**：同一模板序列化为单文件：CSS 内联、图片转 data URI、去掉脚本与发音按钮。
- 网页模式的在线词典默认不导出（只有纯文本）。
- 页脚固定：「由 Lens 词镜导出。释义内容版权归各词典所有，仅供个人学习使用。」

---

## 12. 批量导出（M7）

### 12.1 输入

- 来源：历史（时间范围 + 多选）、粘贴词表、生词本（按标签）。
- 粘贴解析：分隔符 `\n , ; ， ； 、 \t`；去首尾标点；按 lemma 去重（Ran → run）；短语保留整体；非法条目（含路径字符、长度 > 64、纯数字）单列不进任务；单批上限 2000（可调）。
- 历史中的常见虚词（the、a、of…停用词表）默认取消勾选。

### 12.2 预检

估算用时（本地 ~0.3s/词，在线未缓存 ~1s/词）、文件大小（PDF ~150KB/词）、目标目录可写、磁盘剩余 > 估算 × 2、在线缓存命中数。

### 12.3 流水线

```
items(SQLite) ─► [查词 worker × 4] ─► chan(8) ─► [渲染 worker × 1] ─► chan(16) ─► [写盘 worker × 1]
                     │ 在线：governor 令牌桶 2 req/s/源                  │ .part → rename
                     │ 超时 3s，重试 2 次（退避 0.5s/1.5s）                │ 每完成一项更新 batch_item
```

- 有界 channel 形成背压：渲染跟不上时查词自动阻塞，内存不会堆积。
- 每个词的结果写盘后立即 drop。
- 渲染窗口每 150 个词或进程内存 > 800MB（`GetProcessMemoryInfo`，含 WebView2 子进程）时销毁重建。
- 单个词的任何错误都被捕获，记录到 `batch_item.note`，继续下一个。某在线源连续失败 5 次 → 本批跳过该源并标注。
- 文件名：去掉 `\ / : * ? " < > |`、控制字符、首尾空格和点；避开 `CON PRN AUX NUL COM1-9 LPT1-9`；`résumé` → `resume`（NFKD 去变音）；长度 ≤ 120；重名加 `-2`、`-3`。
- 合并模式：一个文件带书签目录；超过 300 词自动分卷。
- 导出期间 `SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED)` 防睡眠，结束恢复。
- 完成后生成 `index.html`（总目录）和 `failed.csv`。
- 启动时 `list_unfinished_batches()`，有则提示「继续上次的导出」，已完成项跳过。
- 进度事件节流 200ms；前端列表虚拟滚动。

---

## 13. 在线词典（M6）

```rust
#[async_trait]
pub trait OnlineProvider: Send + Sync {
    fn id(&self) -> &'static str;
    fn mode(&self) -> OnlineMode;            // Api | WebView
    async fn lookup(&self, lemma: &str) -> Result<OnlineEntry>; // Api 模式
    fn page_url(&self, lemma: &str) -> Url;  // WebView 模式
}
```

- **API 模式**：有官方接口的站点（用户自填 Key，存 Windows 凭据管理器，不写进 settings.json）。结构化结果可参与复制和导出。
- **网页模式**：没有接口的站点，在卡片内嵌只读 WebView 加载原页面（独立 profile、禁弹窗、禁下载），只能复制纯文本，不参与导出和聚合。
- **不在核心里实现抓取解析器**；发布前逐个确认各站服务条款。
- 触发方式（全局默认，可按词典覆盖）：每次都查 / 本地无结果时才查（默认）/ 手动。
- 缓存：`online_cache`，默认 30 天；离线时显示缓存并标注获取时间。

---

## 14. 设置（`settings.json`）

```json
{
  "appearance": { "theme": "paper", "followSystem": false, "lightTheme": "paper", "darkTheme": "dark",
                  "accent": "blue", "glossFontSize": 17, "glossFont": "serif" },
  "layout": { "preset": "compact", "showNav": false, "showSenseAtlas": false, "showContext": false,
              "showStatusBar": false, "showJumpBar": true, "cardsExpanded": "first2" },
  "immersive": { "hideTitleBar": true, "hideStatusBar": true, "hideSidebars": true, "hideCardChrome": true,
                 "borderless": true, "revealOnHover": true, "fullscreen": false, "width": 720,
                 "theme": null, "onStartup": false },
  "capture": { "hotkey": "Ctrl+Alt+Space", "doubleCopy": true, "ocrFallback": false, "timeoutMs": 150 },
  "online": { "trigger": "localMiss", "timeoutMs": 3000, "cacheDays": 30 },
  "copy": { "defaultFormat": "md", "appendSource": true, "includeExamples": true,
            "inlineHtmlStyles": true, "mdImagesAsLinks": true },
  "export": { "layout": "cards", "style": "paperWhite", "paper": "a4", "dir": null },
  "batch": { "maxWords": 2000, "lookupConcurrency": 4, "renderRecycleEvery": 150,
             "memLimitMb": 800, "mergeVolumeSize": 300 },
  "activeProfile": "reading"
}
```

读写走 Rust（`serde` + 原子写：先写临时文件再 rename），前端用 zustand 镜像，改动后 debounce 300ms 保存。

---

## 15. 质量要求

- **崩溃防护**：Rust 侧所有命令返回 `Result`，不 `unwrap`；`std::panic::set_hook` 写日志；每个后台任务用 `catch_unwind` 包裹；前端顶层 ErrorBoundary。
- **日志**：`%APPDATA%/Lens/logs/`，按天滚动，保留 7 天；不记录查词内容以外的个人信息。
- **测试**：
  - Rust：每种词典格式至少一个自制小词典 fixture（`fixtures/`）；断句用例（缩写、小数、引号、中英混排、PDF 断行）；文件名清洗用例；批量导出的断点续传（模拟中途 kill）。
  - 前端：Vitest 测 Markdown 转换、粘贴解析；Playwright 冒烟测查词主流程。
- **性能基准**（`cargo bench` 或简单计时测试）：查词 p95 < 50ms；弹窗出现 < 100ms；导入 100MB mdx < 20s。
- **无障碍**：文字对比度 ≥ 4.5:1（主题表已满足）；所有操作可键盘完成；图标按钮有 `aria-label`。

---

## 16. 风险与待定

| 项 | 说明 |
|---|---|
| mdx 变体多 | 不同生成器版本细节不一致；准备 5–10 部不同来源的词典做兼容测试，解析失败给出清晰提示，不崩溃 |
| UIA 覆盖率 | 部分应用（某些 PDF 阅读器、Electron 应用、游戏）拿不到文本；用剪贴板和 OCR 兜底 |
| 低级键盘钩子 | 回调必须极快，否则系统会把钩子摘掉；只做计时，逻辑放到别的线程 |
| 在线词典条款 | 只用官方 API 或原页面嵌入；发布前逐站确认 |
| 版权 | 不随软件分发任何商业词典；导出文件附版权说明 |
| 词形还原表来源 | 需选用许可清楚的数据源 |

---

## 17. 建议的开发顺序（给 Claude Code 的第一批任务）

1. 初始化 Tauri 2 + React + TS 项目，按第 4 节建目录；加入 SQLite 迁移（第 6 节）和 `tokens.css`（第 9.5 节，先做 paper/light/dark）。
2. 实现 `DictSource` trait 与 JSON 词典 + FST 索引，写单元测试——用最简单的格式先跑通「导入 → 建索引 → 查词」。
3. 实现 StarDict，再实现 mdx/mdd（最复杂，单独一轮）。
4. 主窗口简洁单栏布局：搜索框 + 词典卡片（Shadow DOM 渲染 + `lens://` 资源协议）。
5. 全局快捷键 + 常驻弹窗 + UIA 取词 + 断句。
6. 历史、生词本、复制、单词导出、设置、沉浸模式。

每一步完成后跑测试，再进入下一步。
