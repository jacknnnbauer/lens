-- 英文怎么说的缓存：同一意思、同一对象描述、同一模型只请求一次（audience 不指定时为空串）
CREATE TABLE expression (
  input      TEXT NOT NULL,
  audience   TEXT NOT NULL,
  model      TEXT NOT NULL,
  payload    TEXT NOT NULL,   -- JSON：items（五种风格）
  created_at INTEGER NOT NULL,
  PRIMARY KEY (input, audience, model)
);
