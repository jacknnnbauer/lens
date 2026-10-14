-- AI 双语例句缓存：同一个词、同一个模型只生成一次（可在界面上「重新生成」覆盖）。
CREATE TABLE ai_examples (
  word       TEXT NOT NULL,
  model      TEXT NOT NULL,
  payload    TEXT NOT NULL,              -- JSON：[{en, zh, collocation}]
  created_at INTEGER NOT NULL,
  PRIMARY KEY (word, model)
);
