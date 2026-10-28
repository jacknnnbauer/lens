-- 阅读：存进 Lens 的文章（Markdown 原文）。progress 是上次读到的位置（0–1）
CREATE TABLE article (
  id          INTEGER PRIMARY KEY,
  title       TEXT NOT NULL,
  body        TEXT NOT NULL,
  source_path TEXT,                     -- 从文件导入时的原路径（只记录，不回写）
  words       INTEGER NOT NULL DEFAULT 0,
  progress    REAL NOT NULL DEFAULT 0,
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL,
  opened_at   INTEGER
);
CREATE INDEX article_opened ON article (opened_at);
