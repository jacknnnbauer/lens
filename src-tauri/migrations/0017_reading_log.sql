-- 阅读周报：每天每篇的阅读时长（秒）和读过的词数（估算）。不加外键：删了文章，当时的阅读记录照样算进周报
CREATE TABLE reading_log (
  day        TEXT NOT NULL,              -- 本地日期 YYYY-MM-DD
  article_id INTEGER NOT NULL,
  seconds    INTEGER NOT NULL DEFAULT 0,
  words      INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (day, article_id)
);
