-- 长难句拆解的缓存：同一句、同一模型只请求一次
CREATE TABLE sentence_breakdown (
  sentence   TEXT NOT NULL,
  model      TEXT NOT NULL,
  payload    TEXT NOT NULL,   -- JSON：translation / skeleton / parts / points
  created_at INTEGER NOT NULL,
  PRIMARY KEY (sentence, model)
);
