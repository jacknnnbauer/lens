-- 英文怎么说 · 更多说法的缓存：同一中文说法、同一模型只请求一次（和对象无关）
CREATE TABLE expression_entry (
  input      TEXT NOT NULL,
  model      TEXT NOT NULL,
  payload    TEXT NOT NULL,   -- JSON：groups、dialogue、tip
  created_at INTEGER NOT NULL,
  PRIMARY KEY (input, model)
);
