-- AI 调用记账：每次调用记下用了多少 token、按当时价格估算的花费（美元）。设置 › AI 例句里看汇总。
CREATE TABLE ai_usage (
  id             INTEGER PRIMARY KEY,
  provider       TEXT NOT NULL,           -- claude | deepseek
  model          TEXT NOT NULL,
  word           TEXT NOT NULL,
  input_tokens   INTEGER NOT NULL,        -- 不含缓存命中部分
  cached_tokens  INTEGER NOT NULL,        -- 缓存命中的输入
  output_tokens  INTEGER NOT NULL,
  cost_usd       REAL NOT NULL,
  created_at     INTEGER NOT NULL
);
CREATE INDEX ai_usage_time ON ai_usage (created_at);
