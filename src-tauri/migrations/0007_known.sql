-- 文章生词扫描里标了「认识」的词（原形）：以后扫描不再当生词
CREATE TABLE known_word (
  lemma       TEXT PRIMARY KEY,
  created_at  INTEGER NOT NULL
);
