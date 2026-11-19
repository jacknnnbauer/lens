-- 阅读批注：一段文字的彩色标注，可附笔记。按原文 + 前后文定位（文章能编辑、书能重新导入，只记位置会错位）
CREATE TABLE annotation (
  id         INTEGER PRIMARY KEY,
  article_id INTEGER NOT NULL REFERENCES article(id) ON DELETE CASCADE,
  chapter    INTEGER NOT NULL DEFAULT 0,   -- 第几章（普通文章为 0）
  quote      TEXT NOT NULL,                -- 标注的原文
  prefix     TEXT NOT NULL DEFAULT '',     -- 前面最多 32 个字
  suffix     TEXT NOT NULL DEFAULT '',     -- 后面最多 32 个字
  color      TEXT NOT NULL,                -- yellow / green / blue / pink
  note       TEXT,                         -- 笔记（没有为 NULL）
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE INDEX annotation_article ON annotation (article_id, chapter, created_at);
