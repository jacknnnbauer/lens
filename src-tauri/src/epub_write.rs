//! 把别的格式（mobi / azw3、PDF 重排）的内容写成一个最小的 EPUB2（OPF + NCX + 章节 XHTML + 图片），
//! 存进书库后就和普通 EPUB 一样阅读。

use std::fs::File;
use std::io::Write;
use std::path::Path;

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::error::{Error, Result};

/// 一章：标题（目录用）和 XHTML `<body>` 里面的内容。
/// 章节文件放在 `OEBPS/text/` 下，所以正文里引用图片要写 `../images/xxx`。
#[derive(Debug, Clone, PartialEq)]
pub struct SourceChapter {
    pub title: String,
    pub body: String,
}

/// 书里的图片：`name` 是章节里引用的路径（相对书根，例如 `images/00012.jpg`）
#[derive(Debug, Clone, PartialEq)]
pub struct SourceImage {
    pub name: String,
    pub mime: String,
    pub data: Vec<u8>,
}

/// 目录项：指向第几章（从 0 起），可带章内锚点
#[derive(Debug, Clone, PartialEq)]
pub struct SourceToc {
    pub title: String,
    pub level: i64,
    pub chapter: usize,
    pub anchor: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct BookSource {
    pub title: Option<String>,
    pub author: Option<String>,
    pub chapters: Vec<SourceChapter>,
    pub images: Vec<SourceImage>,
    pub toc: Vec<SourceToc>,
    /// 封面图片的 `name`
    pub cover: Option<String>,
}

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

fn chapter_file(i: usize) -> String {
    format!("text/c{:04}.xhtml", i + 1)
}

fn zip_err(e: zip::result::ZipError) -> Error {
    Error::Other(e.to_string())
}

fn opf(src: &BookSource) -> String {
    let mut m = String::new();
    m.push_str(r#"<item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>"#);
    for i in 0..src.chapters.len() {
        m.push_str(&format!(r#"<item id="c{i}" href="{}" media-type="application/xhtml+xml"/>"#, chapter_file(i)));
    }
    let mut cover_id = None;
    for (i, img) in src.images.iter().enumerate() {
        m.push_str(&format!(r#"<item id="img{i}" href="{}" media-type="{}"/>"#, esc(&img.name), esc(&img.mime)));
        if src.cover.as_deref() == Some(img.name.as_str()) {
            cover_id = Some(format!("img{i}"));
        }
    }
    let spine: String = (0..src.chapters.len()).map(|i| format!(r#"<itemref idref="c{i}"/>"#)).collect();
    let mut meta = String::new();
    meta.push_str(&format!("<dc:title>{}</dc:title>", esc(src.title.as_deref().unwrap_or("Untitled"))));
    if let Some(a) = &src.author {
        meta.push_str(&format!("<dc:creator>{}</dc:creator>", esc(a)));
    }
    meta.push_str("<dc:language>en</dc:language><dc:identifier id=\"uid\">lens-converted</dc:identifier>");
    if let Some(id) = cover_id {
        meta.push_str(&format!(r#"<meta name="cover" content="{id}"/>"#));
    }
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?><package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="uid"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/">{meta}</metadata><manifest>{m}</manifest><spine toc="ncx">{spine}</spine></package>"#
    )
}

fn ncx(src: &BookSource) -> String {
    // 没有目录：每章一项
    let generated: Vec<SourceToc>;
    let toc = if src.toc.is_empty() {
        generated = src.chapters.iter().enumerate().map(|(i, c)| SourceToc { title: c.title.clone(), level: 0, chapter: i, anchor: None }).collect();
        &generated
    } else {
        &src.toc
    };
    let mut out = String::new();
    let mut open: Vec<i64> = Vec::new();
    for (n, t) in toc.iter().filter(|t| t.chapter < src.chapters.len()).enumerate() {
        // 层级只能一级一级往下走
        let level = t.level.clamp(0, open.last().map_or(0, |l| l + 1));
        while open.last().is_some_and(|&l| l >= level) {
            out.push_str("</navPoint>");
            open.pop();
        }
        let href = match &t.anchor {
            Some(a) => format!("{}#{}", chapter_file(t.chapter), a),
            None => chapter_file(t.chapter),
        };
        out.push_str(&format!(r#"<navPoint id="n{n}" playOrder="{}"><navLabel><text>{}</text></navLabel><content src="{}"/>"#, n + 1, esc(&t.title), esc(&href)));
        open.push(level);
    }
    for _ in open {
        out.push_str("</navPoint>");
    }
    format!(r#"<?xml version="1.0" encoding="utf-8"?><ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1"><head><meta name="dtb:uid" content="lens-converted"/></head><docTitle><text>{}</text></docTitle><navMap>{out}</navMap></ncx>"#, esc(src.title.as_deref().unwrap_or("Untitled")))
}

fn chapter_xhtml(c: &SourceChapter) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>{}</title></head><body>{}</body></html>"#,
        esc(&c.title),
        c.body
    )
}

/// 写成 EPUB：先写 `<dest>.tmp` 再改名，写到一半退出不会留下坏文件
pub fn write_epub(dest: &Path, src: &BookSource) -> Result<()> {
    let tmp = dest.with_extension("tmp");
    {
        let mut z = ZipWriter::new(File::create(&tmp)?);
        let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        z.start_file("mimetype", stored).map_err(zip_err)?;
        z.write_all(b"application/epub+zip")?;
        let container = r#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#;
        for (name, text) in [("META-INF/container.xml", container.to_string()), ("OEBPS/content.opf", opf(src)), ("OEBPS/toc.ncx", ncx(src))] {
            z.start_file(name, deflated).map_err(zip_err)?;
            z.write_all(text.as_bytes())?;
        }
        for (i, c) in src.chapters.iter().enumerate() {
            z.start_file(format!("OEBPS/{}", chapter_file(i)), deflated).map_err(zip_err)?;
            z.write_all(chapter_xhtml(c).as_bytes())?;
        }
        for img in &src.images {
            // 图片本来就压缩过，再压没意义
            z.start_file(format!("OEBPS/{}", img.name), stored).map_err(zip_err)?;
            z.write_all(&img.data)?;
        }
        z.finish().map_err(zip_err)?;
    }
    std::fs::rename(&tmp, dest)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::epub;

    fn sample() -> BookSource {
        BookSource {
            title: Some("Alice & <Friends> \"Wonder\"".into()),
            author: Some("Lewis Carroll".into()),
            chapters: vec![
                SourceChapter { title: "Down the Rabbit-Hole".into(), body: "<h2>Down the Rabbit-Hole</h2><p>Alice was beginning to get very tired.</p>".into() },
                SourceChapter { title: "The Pool of Tears".into(), body: r#"<h2 id="pool">The Pool of Tears</h2><p>Curiouser &amp; curiouser!</p><img src="../images/a.png" alt=""/>"#.into() },
                SourceChapter { title: "A Caucus-Race".into(), body: "<p>They were indeed a queer-looking party.</p>".into() },
            ],
            images: vec![SourceImage { name: "images/a.png".into(), mime: "image/png".into(), data: vec![137, 80, 78, 71, 1, 2, 3] }],
            toc: vec![
                SourceToc { title: "Down the Rabbit-Hole".into(), level: 0, chapter: 0, anchor: None },
                SourceToc { title: "The Pool of Tears".into(), level: 0, chapter: 1, anchor: None },
                SourceToc { title: "Tears again".into(), level: 1, chapter: 1, anchor: Some("pool".into()) },
                SourceToc { title: "A Caucus-Race".into(), level: 0, chapter: 2, anchor: None },
            ],
            cover: Some("images/a.png".into()),
        }
    }

    #[test]
    fn round_trips_through_epub_parse() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let p = dir.path().join("out.epub");
        let src = sample();
        write_epub(&p, &src)?;
        let book = epub::parse(&p)?;
        assert_eq!(book.title.as_deref(), src.title.as_deref());
        assert_eq!(book.author.as_deref(), Some("Lewis Carroll"));
        let titles: Vec<&str> = book.chapters.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(titles, ["Down the Rabbit-Hole", "The Pool of Tears", "A Caucus-Race"]);
        let toc: Vec<(&str, i64, i64, Option<&str>)> = book.toc.iter().map(|t| (t.title.as_str(), t.level, t.chapter, t.anchor.as_deref())).collect();
        assert_eq!(toc, [("Down the Rabbit-Hole", 0, 0, None), ("The Pool of Tears", 0, 1, None), ("Tears again", 1, 1, Some("pool")), ("A Caucus-Race", 0, 2, None)]);
        assert!(epub::read_chapter(&p, &book.chapters[1].href)?.contains("Curiouser &amp; curiouser!"));
        assert_eq!(epub::read_file(&p, "OEBPS/images/a.png")?, Some(vec![137, 80, 78, 71, 1, 2, 3]));
        assert!(!dir.path().join("out.epub.tmp").exists() && !dir.path().join("out.tmp").exists());
        Ok(())
    }

    #[test]
    fn escapes_title_and_chapter_titles() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let p = dir.path().join("x.epub");
        let mut src = sample();
        src.toc.clear();
        src.chapters[0].title = "Tom & \"Jerry\" <1>".into();
        src.chapters[0].body = "<p>No heading.</p>".into();
        write_epub(&p, &src)?;
        let book = epub::parse(&p)?;
        assert_eq!(book.title.as_deref(), Some("Alice & <Friends> \"Wonder\""));
        assert_eq!(book.chapters[0].title, "Tom & \"Jerry\" <1>", "没有目录时按章节标题生成目录");
        Ok(())
    }
}
