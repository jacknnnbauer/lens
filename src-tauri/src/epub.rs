//! EPUB 解析：书脊（章节顺序）、目录（EPUB3 nav / EPUB2 NCX）、书名作者，以及按需读章节和图片。
//! 版式和样式不用书里的，正文在前端按阅读页的排版显示。

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use percent_encoding::percent_decode_str;
use serde::{Deserialize, Serialize};
use zip::ZipArchive;

use crate::db::articles::{count_words, decode_file};
use crate::error::{Error, Result};

/// 压缩包里单个文件最多读这么多（防压缩炸弹）
const MAX_ENTRY: u64 = 128 * 1024 * 1024;
/// 只用来混淆字体的加密算法：正文没加密，不算 DRM
const FONT_OBFUSCATION: [&str; 2] = ["http://www.idpf.org/2008/embedding", "http://ns.adobe.com/pdf/enc#RC"];

#[derive(Debug, Clone, PartialEq)]
pub struct Chapter {
    pub href: String,
    pub title: String,
    pub words: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TocItem {
    pub title: String,
    /// 层级，从 0 开始
    pub level: i64,
    /// 指向第几章
    pub chapter: i64,
    pub anchor: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Book {
    pub title: Option<String>,
    pub author: Option<String>,
    pub chapters: Vec<Chapter>,
    pub toc: Vec<TocItem>,
    /// 开头一段纯文字，列表预览用
    pub preview: String,
}

type Archive = ZipArchive<BufReader<File>>;

fn bad_file() -> Error {
    Error::Other(crate::i18n::t("这不是有效的 EPUB 文件", "This isn't a valid EPUB file").into())
}

fn open_archive(path: &Path) -> Result<Archive> {
    ZipArchive::new(BufReader::new(File::open(path)?)).map_err(|_| bad_file())
}

/// 读压缩包里的文件。路径大小写对不上时再找一遍（有的书写法不一致）
fn read_entry(zip: &mut Archive, name: &str) -> Option<Vec<u8>> {
    let actual = if zip.index_for_name(name).is_some() {
        name.to_string()
    } else {
        zip.file_names().filter_map(std::result::Result::ok).find(|n| n.eq_ignore_ascii_case(name))?.into_owned()
    };
    let entry = zip.by_name(&actual).ok()?;
    let mut out = Vec::new();
    entry.take(MAX_ENTRY).read_to_end(&mut out).ok()?;
    Some(out)
}

fn read_text(zip: &mut Archive, name: &str) -> Option<String> {
    read_entry(zip, name).map(|b| decode_file(&b))
}

fn parse_xml(text: &str) -> Option<roxmltree::Document<'_>> {
    roxmltree::Document::parse_with_options(text, roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() }).ok()
}

/// 相对路径解析成压缩包里的路径：相对 `base`（某个文件的路径），解码 %XX，去掉 `.` 和 `..`，分开 `#锚点`
pub fn resolve(base: &str, href: &str) -> (String, Option<String>) {
    let (path, anchor) = match href.split_once('#') {
        Some((p, a)) => (p, Some(a)),
        None => (href, None),
    };
    let decode = |s: &str| percent_decode_str(s).decode_utf8_lossy().into_owned();
    let path = decode(path);
    if path.is_empty() {
        return (base.to_string(), anchor.filter(|a| !a.is_empty()).map(decode));
    }
    let mut parts: Vec<&str> = if path.starts_with('/') { Vec::new() } else { base.split('/').collect() };
    if !path.starts_with('/') {
        parts.pop();
    }
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    (parts.join("/"), anchor.filter(|a| !a.is_empty()).map(decode))
}

/// 用了 DRM 就读不出正文：encryption.xml 里有字体混淆以外的算法
fn has_drm(zip: &mut Archive) -> bool {
    let Some(text) = read_text(zip, "META-INF/encryption.xml") else { return false };
    let Some(doc) = parse_xml(&text) else { return true };
    let drm = doc
        .descendants()
        .filter(|n| n.tag_name().name() == "EncryptionMethod")
        .filter_map(|n| n.attribute("Algorithm"))
        .any(|a| !FONT_OBFUSCATION.contains(&a));
    drm
}

fn child_text(node: roxmltree::Node) -> String {
    let joined: Vec<&str> = node.descendants().filter(|n| n.is_text()).filter_map(|n| n.text()).collect();
    joined.join("").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// EPUB3 目录：`<nav epub:type="toc">` 里嵌套的 ol / li
fn nav_entries(text: &str) -> Vec<(String, String, i64)> {
    fn walk(ol: roxmltree::Node, level: i64, out: &mut Vec<(String, String, i64)>) {
        for li in ol.children().filter(|n| n.tag_name().name() == "li") {
            if let Some(a) = li.children().find(|n| n.tag_name().name() == "a") {
                let title = child_text(a);
                if let (Some(href), false) = (a.attribute("href"), title.is_empty()) {
                    out.push((title, href.to_string(), level));
                }
            }
            for sub in li.children().filter(|n| n.tag_name().name() == "ol") {
                walk(sub, level + 1, out);
            }
        }
    }
    let Some(doc) = parse_xml(text) else { return Vec::new() };
    let navs: Vec<_> = doc.descendants().filter(|n| n.tag_name().name() == "nav").collect();
    let is_toc = |n: &roxmltree::Node| n.attributes().any(|a| a.name() == "type" && a.value().split_whitespace().any(|v| v == "toc"));
    let Some(nav) = navs.iter().find(|n| is_toc(n)).or(navs.first()) else { return Vec::new() };
    let mut out = Vec::new();
    for ol in nav.children().filter(|n| n.tag_name().name() == "ol") {
        walk(ol, 0, &mut out);
    }
    out
}

/// EPUB2 目录：NCX 里嵌套的 navPoint
fn ncx_entries(text: &str) -> Vec<(String, String, i64)> {
    fn walk(parent: roxmltree::Node, level: i64, out: &mut Vec<(String, String, i64)>) {
        for p in parent.children().filter(|n| n.tag_name().name() == "navPoint") {
            let title = p.children().find(|n| n.tag_name().name() == "navLabel").map(child_text).unwrap_or_default();
            let src = p.children().find(|n| n.tag_name().name() == "content").and_then(|n| n.attribute("src"));
            if let (Some(src), false) = (src, title.is_empty()) {
                out.push((title, src.to_string(), level));
            }
            walk(p, level + 1, out);
        }
    }
    let Some(doc) = parse_xml(text) else { return Vec::new() };
    let mut out = Vec::new();
    if let Some(map) = doc.descendants().find(|n| n.tag_name().name() == "navMap") {
        walk(map, 0, &mut out);
    }
    out
}

fn entity(name: &str) -> Option<char> {
    if let Some(n) = name.strip_prefix('#') {
        let code = match n.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => n.parse().ok()?,
        };
        return char::from_u32(code);
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" | "ldquo" | "rdquo" => '"',
        "apos" | "rsquo" | "lsquo" => '\'',
        "mdash" | "ndash" => '-',
        "hellip" => '…',
        _ => ' ',
    })
}

fn push_text(out: &mut String, text: &str) {
    let mut rest = text;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i + 1..];
        match tail.find(';').filter(|&e| e <= 10).and_then(|e| entity(&tail[..e]).map(|c| (e, c))) {
            Some((e, c)) => {
                out.push(c);
                rest = &tail[e + 1..];
            }
            None => {
                out.push('&');
                rest = tail;
            }
        }
    }
    out.push_str(rest);
}

/// 章节的纯文字（词数、预览、找标题用）：去掉标签和 head / script / style，解开实体
pub fn plain_of_html(html: &str) -> String {
    strip_tags(html, false)
}

/// 同上，但段落、标题、列表项、换行之间换行：生词扫描按段落断句，标题不会和下一段的第一句连在一起
pub fn text_of_html(html: &str) -> String {
    strip_tags(html, true)
}

/// 段落一类的块级元素
fn is_block(name: &str) -> bool {
    matches!(name, "p" | "div" | "br" | "li" | "tr" | "blockquote" | "section" | "article" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "pre" | "hr" | "dd" | "dt" | "td" | "th")
}

fn strip_tags(html: &str, lines: bool) -> String {
    let mut out = String::with_capacity(html.len() / 2);
    let mut rest = html;
    while let Some(lt) = rest.find('<') {
        push_text(&mut out, &rest[..lt]);
        let tail = &rest[lt..];
        let Some(gt) = tail.find('>') else { return out };
        let name: String = tail[1..].chars().take_while(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
        // 这些标签连内容一起跳过
        if matches!(name.as_str(), "head" | "script" | "style") && !tail[..gt].ends_with('/') {
            let close = format!("</{name}");
            let end = find_ci(tail, &close).and_then(|i| tail[i..].find('>').map(|j| i + j + 1));
            match end {
                Some(end) => rest = &tail[end..],
                None => return out,
            }
            continue;
        }
        let tag: String = tail[1..].trim_start_matches('/').chars().take_while(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
        out.push(if lines && is_block(&tag) { '\n' } else { ' ' });
        rest = &tail[gt + 1..];
    }
    push_text(&mut out, rest);
    out
}

/// 不分大小写找 ASCII 字符串，返回的位置总在字符边界上
fn find_ci(hay: &str, needle: &str) -> Option<usize> {
    let (h, n) = (hay.as_bytes(), needle.as_bytes());
    if n.is_empty() || h.len() < n.len() {
        return None;
    }
    (0..=h.len() - n.len()).find(|&i| h[i..i + n.len()].eq_ignore_ascii_case(n))
}

/// 章节里第一个 h1–h3 的文字
fn first_heading(html: &str) -> Option<String> {
    let (start, tag) = ["h1", "h2", "h3"].iter().filter_map(|t| find_ci(html, &format!("<{t}")).map(|i| (i, *t))).min()?;
    let open_end = start + html[start..].find('>')? + 1;
    let end = open_end + find_ci(&html[open_end..], &format!("</{tag}"))?;
    let text = plain_of_html(&html[open_end..end]).split_whitespace().collect::<Vec<_>>().join(" ");
    (!text.is_empty() && text.chars().count() <= 120).then_some(text)
}

/// 目录项（标题、href、层级），href 相对目录文件
fn read_toc_entries(zip: &mut Archive, nav: Option<String>, ncx: Option<String>) -> Vec<(String, String, i64, String)> {
    // 优先 EPUB3 nav，解析不了或是空的再用 NCX
    if let Some(base) = nav {
        if let Some(text) = read_text(zip, &base) {
            let e = nav_entries(&text);
            if !e.is_empty() {
                return e.into_iter().map(|(t, h, l)| (t, h, l, base.clone())).collect();
            }
        }
    }
    if let Some(base) = ncx {
        if let Some(text) = read_text(zip, &base) {
            return ncx_entries(&text).into_iter().map(|(t, h, l)| (t, h, l, base.clone())).collect();
        }
    }
    Vec::new()
}

pub fn parse(path: &Path) -> Result<Book> {
    let mut zip = open_archive(path)?;
    if has_drm(&mut zip) {
        return Err(Error::Other(crate::i18n::t("这本书有 DRM 加密，读不出正文", "This book is DRM-protected, so its text can't be read").into()));
    }
    let container = read_text(&mut zip, "META-INF/container.xml").ok_or_else(bad_file)?;
    let opf_path = parse_xml(&container)
        .and_then(|d| d.descendants().find(|n| n.tag_name().name() == "rootfile").and_then(|n| n.attribute("full-path")).map(str::to_string))
        .ok_or_else(bad_file)?;
    let opf_text = read_text(&mut zip, &opf_path).ok_or_else(bad_file)?;
    let opf = parse_xml(&opf_text).ok_or_else(bad_file)?;

    let meta = |tag: &str| {
        opf.descendants()
            .find(|n| n.tag_name().name() == "metadata")
            .and_then(|m| m.children().find(|n| n.tag_name().name() == tag))
            .map(child_text)
            .filter(|t| !t.is_empty())
    };
    let (title, author) = (meta("title"), meta("creator"));

    struct Item<'a> {
        href: String,
        media: &'a str,
        props: &'a str,
    }
    let mut items: HashMap<&str, Item> = HashMap::new();
    for n in opf.descendants().filter(|n| n.tag_name().name() == "item") {
        if let (Some(id), Some(href)) = (n.attribute("id"), n.attribute("href")) {
            let item = Item { href: resolve(&opf_path, href).0, media: n.attribute("media-type").unwrap_or_default(), props: n.attribute("properties").unwrap_or_default() };
            items.insert(id, item);
        }
    }
    let spine = opf.descendants().find(|n| n.tag_name().name() == "spine").ok_or_else(bad_file)?;

    // 章节正文（各读一遍：词数、标题、预览）
    let mut hrefs: Vec<String> = Vec::new();
    let mut htmls: Vec<String> = Vec::new();
    for r in spine.children().filter(|n| n.tag_name().name() == "itemref") {
        let Some(item) = r.attribute("idref").and_then(|id| items.get(id)) else { continue };
        let lower = item.href.to_ascii_lowercase();
        let is_html = item.media.contains("html") || lower.ends_with("html") || lower.ends_with(".htm");
        if !is_html || hrefs.contains(&item.href) {
            continue;
        }
        if let Some(text) = read_text(&mut zip, &item.href) {
            hrefs.push(item.href.clone());
            htmls.push(text);
        }
    }
    if hrefs.is_empty() {
        return Err(Error::Other(crate::i18n::t("这本书里没有可读的内容", "This book has no readable content").into()));
    }
    let chapter_of: HashMap<&str, i64> = hrefs.iter().enumerate().map(|(i, h)| (h.as_str(), i as i64)).collect();

    let nav = items.values().find(|i| i.props.split_whitespace().any(|p| p == "nav")).map(|i| i.href.clone());
    let ncx = spine.attribute("toc").and_then(|id| items.get(id)).or_else(|| items.values().find(|i| i.media.contains("dtbncx"))).map(|i| i.href.clone());
    let mut toc: Vec<TocItem> = Vec::new();
    for (title, href, level, base) in read_toc_entries(&mut zip, nav, ncx) {
        let (path, anchor) = resolve(&base, &href);
        if let Some(&chapter) = chapter_of.get(path.as_str()) {
            toc.push(TocItem { title, level, chapter, anchor });
        }
    }

    // 多章的书目录只有一项（比如只有「Start」）：等于没有目录
    if toc.len() < 2 {
        toc.clear();
    }

    // 章节标题：目录里指向这一章的第一项 → 正文的第一个标题 → 沿用上一章（一章被切成几个文件时）→ 第 N 节
    let mut chapters = Vec::with_capacity(hrefs.len());
    let mut preview = String::new();
    let mut short_preview = String::new();
    let mut last_title: Option<String> = None;
    for (i, (href, html)) in hrefs.iter().zip(&htmls).enumerate() {
        let text = plain_of_html(html);
        let words = count_words(&text);
        // 预览取第一个像正文的章节，跳过版权页；都很短时才退而用短的
        if preview.is_empty() && words >= 50 {
            let lead: String = text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(400).collect();
            if words >= 500 {
                preview = lead;
            } else if short_preview.is_empty() {
                short_preview = lead;
            }
        }
        let from_toc = toc.iter().find(|t| t.chapter == i as i64).map(|t| t.title.clone());
        if let Some(t) = from_toc.or_else(|| first_heading(html)) {
            last_title = Some(t);
        }
        let title = last_title.clone().unwrap_or_else(|| crate::i18n::t("第 {n} 节", "Section {n}").replace("{n}", &(i + 1).to_string()));
        chapters.push(Chapter { href: href.clone(), title, words });
    }
    // 没有目录：用章节当目录，相邻同名的算一项
    if toc.is_empty() {
        let mut prev: Option<&str> = None;
        for (i, c) in chapters.iter().enumerate() {
            if prev != Some(c.title.as_str()) {
                toc.push(TocItem { title: c.title.clone(), level: 0, chapter: i as i64, anchor: None });
            }
            prev = Some(c.title.as_str());
        }
    }
    if preview.is_empty() {
        preview = short_preview;
    }
    Ok(Book { title, author, chapters, toc, preview })
}

/// 读一章的原文（XHTML）
pub fn read_chapter(path: &Path, href: &str) -> Result<String> {
    let mut zip = open_archive(path)?;
    read_text(&mut zip, href).ok_or_else(bad_file)
}

/// 读书里的一个文件（图片）
pub fn read_file(path: &Path, inner: &str) -> Result<Option<Vec<u8>>> {
    let mut zip = open_archive(path)?;
    Ok(read_entry(&mut zip, inner))
}

/// 文件内容哈希（FNV-1a 64 位 + 长度）：认出同一本书，也用作数据目录里的文件名
pub fn hash_file(path: &Path) -> Result<String> {
    let mut reader = BufReader::with_capacity(1 << 20, File::open(path)?);
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut len: u64 = 0;
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        for &b in &buf[..n] {
            h = (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
        }
        len += n as u64;
    }
    Ok(format!("{h:016x}-{len:x}"))
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    pub fn write_epub(path: &Path, files: &[(&str, &str)]) -> Result<()> {
        let mut z = zip::ZipWriter::new(File::create(path)?);
        for (name, text) in files {
            z.start_file(*name, SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored)).map_err(|e| Error::Other(e.to_string()))?;
            z.write_all(text.as_bytes())?;
        }
        z.finish().map_err(|e| Error::Other(e.to_string()))?;
        Ok(())
    }

    const CONTAINER: &str = r#"<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#;
    const OPF: &str = r#"<package xmlns="http://www.idpf.org/2007/opf" xmlns:dc="http://purl.org/dc/elements/1.1/"><metadata><dc:title>The Rusty Bike</dc:title><dc:creator>A. Writer</dc:creator></metadata>
<manifest><item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/><item id="c1" href="text/one%20a.xhtml" media-type="application/xhtml+xml"/><item id="c2" href="text/two.xhtml" media-type="application/xhtml+xml"/><item id="c3" href="text/three.xhtml" media-type="application/xhtml+xml"/><item id="pic" href="img/a.png" media-type="image/png"/></manifest>
<spine toc="ncx"><itemref idref="c1"/><itemref idref="c2"/><itemref idref="c3"/><itemref idref="missing"/></spine></package>"#;
    const NCX: &str = r#"<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/"><navMap><navPoint><navLabel><text>Part One</text></navLabel><content src="text/one%20a.xhtml"/><navPoint><navLabel><text>Rain</text></navLabel><content src="text/one%20a.xhtml#rain"/></navPoint></navPoint><navPoint><navLabel><text>Ghost</text></navLabel><content src="text/none.xhtml"/></navPoint></navMap></ncx>"#;
    const ONE: &str = "<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>x y z</title><style>p{color:red}</style></head><body><h1>One</h1><p>It isn&rsquo;t rusty &amp; old.</p></body></html>";
    const TWO: &str = "<html><body><h2>Two <i>Wheels</i></h2><p>More words.</p></body></html>";
    const THREE: &str = "<html><body><p>No heading here.</p></body></html>";

    pub fn sample(dir: &Path) -> Result<std::path::PathBuf> {
        let p = dir.join("book.epub");
        write_epub(
            &p,
            &[
                ("META-INF/container.xml", CONTAINER),
                ("OEBPS/book.opf", OPF),
                ("OEBPS/toc.ncx", NCX),
                ("OEBPS/text/one a.xhtml", ONE),
                ("OEBPS/text/two.xhtml", TWO),
                ("OEBPS/text/three.xhtml", THREE),
                ("OEBPS/img/a.png", "PNG"),
            ],
        )?;
        Ok(p)
    }

    #[test]
    fn parses_spine_toc_and_titles() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let book = parse(&sample(dir.path())?)?;
        assert_eq!((book.title.as_deref(), book.author.as_deref()), (Some("The Rusty Bike"), Some("A. Writer")));
        let titles: Vec<&str> = book.chapters.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(titles, ["Part One", "Two Wheels", "Two Wheels"], "目录 → 正文标题 → 沿用上一章");
        assert_eq!(book.chapters[0].href, "OEBPS/text/one a.xhtml");
        assert_eq!(book.chapters[0].words, 5, "head 不算，实体解开");
        assert_eq!(book.toc.len(), 2, "指向不存在文件的目录项丢掉");
        assert_eq!(book.toc[1], TocItem { title: "Rain".into(), level: 1, chapter: 0, anchor: Some("rain".into()) });
        Ok(())
    }

    #[test]
    fn text_keeps_paragraph_breaks() {
        let html = "<html><head><title>x</title></head><body><h1>Loomings</h1><p>Call me <i>Ishmael</i>.</p><p>Some years ago&mdash;never mind.<br/>How long.</p></body></html>";
        let t = text_of_html(html);
        let lines: Vec<&str> = t.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
        assert_eq!(lines, ["Loomings", "Call me  Ishmael .", "Some years ago-never mind.", "How long."]);
        // 原来的纯文字不变：不换行
        assert!(!plain_of_html(html).contains('\n'));
    }

    #[test]
    fn useless_toc_falls_back_to_chapters() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let p = dir.path().join("b.epub");
        let ncx = r#"<ncx><navMap><navPoint><navLabel><text>Start</text></navLabel><content src="text/one.xhtml"/></navPoint></navMap></ncx>"#;
        let opf = OPF.replace("one%20a.xhtml", "one.xhtml");
        write_epub(&p, &[("META-INF/container.xml", CONTAINER), ("OEBPS/book.opf", &opf), ("OEBPS/toc.ncx", ncx), ("OEBPS/text/one.xhtml", THREE), ("OEBPS/text/two.xhtml", THREE), ("OEBPS/text/three.xhtml", TWO)])?;
        let book = parse(&p)?;
        let titles: Vec<&str> = book.toc.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles.len(), 3, "只有一项的目录不用，也不拿它当章节名：{titles:?}");
        assert_eq!(titles[2], "Two Wheels");
        assert!(titles[0] != "Start" && titles[0] != titles[1]);
        Ok(())
    }

    #[test]
    fn reads_chapters_and_images_on_demand() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let p = sample(dir.path())?;
        assert_eq!(read_chapter(&p, "OEBPS/text/two.xhtml")?, TWO);
        assert_eq!(read_file(&p, "OEBPS/img/a.png")?.as_deref(), Some(&b"PNG"[..]));
        assert_eq!(read_file(&p, "oebps/IMG/a.png")?.as_deref(), Some(&b"PNG"[..]), "大小写不敏感");
        assert_eq!(read_file(&p, "nope.png")?, None);
        Ok(())
    }

    #[test]
    fn nav_document_picks_the_toc() {
        let nav = r#"<html xmlns:epub="http://www.idpf.org/2007/ops"><body><nav epub:type="landmarks"><ol><li><a href="x.xhtml">Land</a></li></ol></nav><nav epub:type="toc"><ol><li><a href="a.xhtml">A</a><ol><li><a href="a.xhtml#b">B <b>bold</b></a></li></ol></li><li><span>No link</span></li></ol></nav></body></html>"#;
        let e = nav_entries(nav);
        assert_eq!(e, vec![("A".to_string(), "a.xhtml".to_string(), 0), ("B bold".to_string(), "a.xhtml#b".to_string(), 1)]);
    }

    #[test]
    fn rejects_bad_files_and_drm() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let junk = dir.path().join("junk.epub");
        std::fs::write(&junk, "not a zip")?;
        assert!(parse(&junk).is_err());
        let drm = dir.path().join("drm.epub");
        let enc = r#"<encryption><EncryptedData><EncryptionMethod Algorithm="http://www.w3.org/2001/04/xmlenc#aes128-cbc"/></EncryptedData></encryption>"#;
        write_epub(&drm, &[("META-INF/container.xml", CONTAINER), ("META-INF/encryption.xml", enc), ("OEBPS/book.opf", OPF)])?;
        assert!(parse(&drm).is_err());
        let font = r#"<encryption><EncryptedData><EncryptionMethod Algorithm="http://www.idpf.org/2008/embedding"/></EncryptedData></encryption>"#;
        let ok = dir.path().join("font.epub");
        write_epub(&ok, &[("META-INF/container.xml", CONTAINER), ("META-INF/encryption.xml", font), ("OEBPS/book.opf", OPF), ("OEBPS/text/two.xhtml", TWO)])?;
        assert!(parse(&ok).is_ok(), "只混淆字体不算 DRM");
        Ok(())
    }

    #[test]
    fn resolves_relative_paths() {
        assert_eq!(resolve("OEBPS/text/a.xhtml", "../img/b%20c.png"), ("OEBPS/img/b c.png".to_string(), None));
        assert_eq!(resolve("a.xhtml", "b.xhtml#n1"), ("b.xhtml".to_string(), Some("n1".to_string())));
        assert_eq!(resolve("x/a.xhtml", "../../../etc"), ("etc".to_string(), None));
        assert_eq!(resolve("x/a.xhtml", "#n2"), ("x/a.xhtml".to_string(), Some("n2".to_string())));
    }

    #[test]
    fn plain_text_handles_entities_and_skipped_tags() {
        let norm = |s: &str| plain_of_html(s).split_whitespace().collect::<Vec<_>>().join(" ");
        assert_eq!(norm("<head><title>t</title></head><p>a &amp; b&#x2019;s</p><script>x</script>"), "a & b’s");
        assert_eq!(norm("x &nope y"), "x &nope y");
        assert_eq!(norm("中<b>文</b>"), "中 文");
        assert_eq!(first_heading("<p>中文</p><H2 class='a'>Two <i>W</i></H2><h1>Later</h1>").as_deref(), Some("Two W"));
    }
}
