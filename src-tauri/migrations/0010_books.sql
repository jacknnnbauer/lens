-- 阅读：电子书（EPUB）。一本书也是 article 里的一行：body 只放开头一段预览，正文和图片按需从 books/<book_hash>.epub 读
ALTER TABLE article ADD COLUMN book_hash TEXT;              -- 文件内容哈希；普通文章为 NULL
ALTER TABLE article ADD COLUMN author TEXT;
ALTER TABLE article ADD COLUMN toc TEXT;                    -- 目录（JSON），可以指到章节里的锚点
ALTER TABLE article ADD COLUMN chapter INTEGER NOT NULL DEFAULT 0;      -- 读到第几章
ALTER TABLE article ADD COLUMN chapter_pos REAL NOT NULL DEFAULT 0;     -- 这一章读到的位置（0–1）；progress 是全书的
CREATE UNIQUE INDEX article_book_hash ON article (book_hash) WHERE book_hash IS NOT NULL;

CREATE TABLE article_chapter (
  article_id INTEGER NOT NULL REFERENCES article (id) ON DELETE CASCADE,
  idx        INTEGER NOT NULL,      -- 书脊（spine）顺序
  href       TEXT NOT NULL,         -- 压缩包里的路径
  title      TEXT NOT NULL,
  words      INTEGER NOT NULL,
  PRIMARY KEY (article_id, idx)
) WITHOUT ROWID;
