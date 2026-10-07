-- 0001 初始结构（SPEC 第 6 节）。已发布的迁移不得修改，新字段另写迁移文件。

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
  profile_id  TEXT NOT NULL REFERENCES profile(id) ON DELETE CASCADE,
  dict_id     TEXT NOT NULL REFERENCES dict(id)    ON DELETE CASCADE,
  enabled     INTEGER NOT NULL DEFAULT 1,
  sort        INTEGER NOT NULL,
  in_popup    INTEGER NOT NULL DEFAULT 1,
  expanded    INTEGER NOT NULL DEFAULT 0,
  query_kinds TEXT NOT NULL DEFAULT 'word,phrase', -- word,phrase,zh,wildcard
  PRIMARY KEY (profile_id, dict_id)
);

-- 语境句（每次划词捕获一条）
CREATE TABLE context (
  id           INTEGER PRIMARY KEY,
  lemma        TEXT NOT NULL,
  surface      TEXT NOT NULL,               -- 句中原形（如 ran）
  sentence     TEXT NOT NULL,
  hl_start     INTEGER NOT NULL,            -- 单词在句中的 UTF-16 偏移，用于高亮
  hl_end       INTEGER NOT NULL,
  app          TEXT,                        -- 进程名，如 msedge.exe
  window_title TEXT,
  url          TEXT,
  file_path    TEXT,
  page         INTEGER,
  method       TEXT NOT NULL,               -- uia|ocr|clipboard|manual
  created_at   INTEGER NOT NULL
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

-- 生词本
CREATE TABLE word (
  id          INTEGER PRIMARY KEY,
  lemma       TEXT NOT NULL UNIQUE,
  brief       TEXT,                         -- 一句话释义（取首选词典第一义项）
  familiarity INTEGER NOT NULL DEFAULT 0,   -- 0..5
  due_at      INTEGER,                      -- 下次复习（M8 接 FSRS）
  created_at  INTEGER NOT NULL
);
CREATE TABLE word_context (
  word_id    INTEGER REFERENCES word(id)    ON DELETE CASCADE,
  context_id INTEGER REFERENCES context(id) ON DELETE CASCADE,
  PRIMARY KEY (word_id, context_id)
);
CREATE TABLE tag (id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE);
CREATE TABLE word_tag (
  word_id INTEGER REFERENCES word(id) ON DELETE CASCADE,
  tag_id  INTEGER REFERENCES tag(id)  ON DELETE CASCADE,
  PRIMARY KEY (word_id, tag_id)
);

-- 在线词典缓存（M6）
CREATE TABLE online_cache (
  provider   TEXT NOT NULL,
  lemma      TEXT NOT NULL,
  payload    TEXT NOT NULL,
  fetched_at INTEGER NOT NULL,
  PRIMARY KEY (provider, lemma)
);

-- 批量导出（M7，断点续传）
CREATE TABLE batch (
  id           TEXT PRIMARY KEY,
  request_json TEXT NOT NULL,
  state        TEXT NOT NULL,               -- running|paused|cancelled|done
  out_dir      TEXT NOT NULL,
  created_at   INTEGER NOT NULL,
  updated_at   INTEGER NOT NULL
);
CREATE TABLE batch_item (
  batch_id TEXT NOT NULL REFERENCES batch(id) ON DELETE CASCADE,
  seq      INTEGER NOT NULL,
  lemma    TEXT NOT NULL,
  state    TEXT NOT NULL,                   -- pending|querying|rendering|done|failed|skipped
  note     TEXT,                            -- 失败原因
  files    TEXT,                            -- 生成的文件名（JSON 数组）
  PRIMARY KEY (batch_id, seq)
);

-- 全文反查（中文查英文）：对每部词典的纯文本释义建 FTS5（可选，导入时勾选）
CREATE VIRTUAL TABLE gloss_fts USING fts5(dict_id UNINDEXED, headword UNINDEXED, text, tokenize='trigram');

-- 默认场景配置组；settings.json 的 activeProfile 默认指向 reading
INSERT INTO profile (id, name, sort) VALUES
  ('reading',   '阅读', 0),
  ('translate', '翻译', 1),
  ('coding',    '编程', 2);
