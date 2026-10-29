-- 阅读：文章在 Lens 里改过正文就记 1。再次从同一个文件打开时，没改过的直接换成文件的新内容，改过的另存一篇
ALTER TABLE article ADD COLUMN edited INTEGER NOT NULL DEFAULT 0;
CREATE INDEX article_source ON article (source_path);
