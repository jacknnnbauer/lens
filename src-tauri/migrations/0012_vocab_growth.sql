-- 词汇量成长：每次测试的结果和每天的词汇量。
-- known_word.source：manual 手动标记 · import 批量导入（旧数据分不清，都算导入，不计入成长曲线）
ALTER TABLE known_word ADD COLUMN source TEXT NOT NULL DEFAULT 'import';

CREATE TABLE vocab_test (
  id            INTEGER PRIMARY KEY,
  taken_at      INTEGER NOT NULL,
  day           TEXT NOT NULL,              -- 本地日期 YYYY-MM-DD
  value         INTEGER NOT NULL,
  low           INTEGER NOT NULL,
  high          INTEGER NOT NULL,
  bank_version  INTEGER NOT NULL,
  excluded      TEXT NOT NULL,              -- JSON：测试时已算进结果的词，之后不再计入「学会」
  asked         TEXT NOT NULL               -- JSON：这次考过的词，重测时避开
);

CREATE TABLE vocab_snapshot (
  day     TEXT PRIMARY KEY,                 -- 本地日期 YYYY-MM-DD
  value   INTEGER NOT NULL,
  gained  INTEGER NOT NULL
);
