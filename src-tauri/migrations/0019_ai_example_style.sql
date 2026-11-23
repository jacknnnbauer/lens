-- AI 双语例句缓存按「长度 + 句型」再分一层：style 为空 = 适中、不限句型（旧缓存都算这一种）
CREATE TABLE ai_examples_new (
  word       TEXT NOT NULL,
  model      TEXT NOT NULL,
  style      TEXT NOT NULL DEFAULT '',
  payload    TEXT NOT NULL,              -- JSON：[{en, zh, collocation, clause}]
  created_at INTEGER NOT NULL,
  PRIMARY KEY (word, model, style)
);
INSERT INTO ai_examples_new (word, model, style, payload, created_at)
  SELECT word, model, '', payload, created_at FROM ai_examples;
DROP TABLE ai_examples;
ALTER TABLE ai_examples_new RENAME TO ai_examples;
