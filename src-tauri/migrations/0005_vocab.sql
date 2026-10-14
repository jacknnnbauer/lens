-- vocabulary.com 真实例句缓存：每个（词, 来源筛选, 起始位置）一页。
CREATE TABLE vocab_examples (
  key        TEXT PRIMARY KEY,           -- 词（小写）\x01 来源代码 \x01 起始位置
  total      INTEGER NOT NULL,           -- 该筛选下的例句总数
  payload    TEXT NOT NULL,              -- JSON：[{text, hl, source, title, author, published, url}]
  fetched_at INTEGER NOT NULL
);
