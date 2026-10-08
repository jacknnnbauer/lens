-- M8 间隔复习（FSRS）：每个词的记忆状态 + 复习记录。
-- review_state：0 新词 · 1 学习中 · 2 复习 · 3 重学；due_at 沿用 0001 的字段。
ALTER TABLE word ADD COLUMN stability REAL;
ALTER TABLE word ADD COLUMN difficulty REAL;
ALTER TABLE word ADD COLUMN reps INTEGER NOT NULL DEFAULT 0;
ALTER TABLE word ADD COLUMN lapses INTEGER NOT NULL DEFAULT 0;
ALTER TABLE word ADD COLUMN last_review INTEGER;
ALTER TABLE word ADD COLUMN review_state INTEGER NOT NULL DEFAULT 0;
CREATE INDEX word_due ON word (review_state, due_at);

CREATE TABLE review_log (
  id           INTEGER PRIMARY KEY,
  word_id      INTEGER NOT NULL REFERENCES word(id) ON DELETE CASCADE,
  rating       INTEGER NOT NULL,              -- 1 忘了 · 2 模糊 · 3 记得 · 4 简单
  state_before INTEGER NOT NULL,
  reviewed_at  INTEGER NOT NULL,
  due_after    INTEGER NOT NULL
);
CREATE INDEX review_log_time ON review_log (reviewed_at);
