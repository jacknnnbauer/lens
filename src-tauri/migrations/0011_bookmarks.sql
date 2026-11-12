-- 阅读：书签。chapter = 第几章（普通文章为 0），pos = 这一章（或整篇）的位置 0–1，label = 书签处的一小段原文
CREATE TABLE bookmark (
  id          INTEGER PRIMARY KEY,
  article_id  INTEGER NOT NULL REFERENCES article (id) ON DELETE CASCADE,
  chapter     INTEGER NOT NULL DEFAULT 0,
  pos         REAL NOT NULL DEFAULT 0,
  label       TEXT NOT NULL,
  created_at  INTEGER NOT NULL
);
CREATE INDEX bookmark_article ON bookmark (article_id, chapter, pos);
