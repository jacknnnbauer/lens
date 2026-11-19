-- PDF 书：pdf_pages = 每章开头是原 PDF 第几页（JSON 数组，从 1 起；不是 PDF 为 NULL）
-- pdf_page = 原版页面看到第几页，pdf_view = 上次用的视图（reflow / pages，NULL 视为 reflow）
ALTER TABLE article ADD COLUMN pdf_pages TEXT;
ALTER TABLE article ADD COLUMN pdf_page INTEGER NOT NULL DEFAULT 0;
ALTER TABLE article ADD COLUMN pdf_view TEXT;
