//! Kindle 电子书（.mobi / .azw3 / .azw）：读出正文、图片、目录、书名作者，转成 `BookSource` 交给 EPUB 写出器。
//! 只读没有 DRM 的书；有 DRM 的直接报错，不尝试解密。

mod huffcdic;
mod indx;
mod lz77;
mod mobi6;
mod palmdb;

use std::path::Path;

use crate::epub_write::{BookSource, SourceImage};
use crate::error::{Error, Result};

pub(crate) use lz77::decompress_palmdoc;

pub fn drm_error() -> Error {
    Error::Other(crate::i18n::t("这本书有 DRM 保护，Lens 打不开。", "This book is DRM-protected, so Lens can't open it.").into())
}

pub fn corrupt_error() -> Error {
    Error::Other(crate::i18n::t("这个文件打不开，可能已损坏。", "This file can't be opened. It may be damaged.").into())
}

/// 读一本 Kindle 书
pub fn read(path: &Path) -> Result<BookSource> {
    let bytes = std::fs::read(path)?;
    read_bytes(&bytes)
}

const NONE: u32 = u32::MAX;

fn be32(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at + 4).map(|s| u32::from_be_bytes(s.try_into().unwrap()))
}

fn be16(b: &[u8], at: usize) -> Option<u16> {
    b.get(at..at + 2).map(|s| u16::from_be_bytes(s.try_into().unwrap()))
}

/// record0（PalmDOC 头 + MOBI 头 + EXTH）。里面的记录号都相对于这一节的 record0
pub(crate) struct Header {
    pub compression: u16,
    pub text_records: usize,
    pub encryption: u16,
    pub codepage: u32,
    pub version: u32,
    pub title: Option<Vec<u8>>,
    pub first_image: u32,
    pub huff: (u32, u32),
    pub extra_flags: u16,
    pub ncx: u32,
    pub fdst: (u32, u32),
    pub frag: u32,
    pub skel: u32,
    pub exth: Vec<(u32, Vec<u8>)>,
}

impl Header {
    pub fn parse(r0: &[u8]) -> Result<Header> {
        let bad = corrupt_error;
        let compression = be16(r0, 0).ok_or_else(bad)?;
        let text_records = usize::from(be16(r0, 8).ok_or_else(bad)?);
        let encryption = be16(r0, 12).ok_or_else(bad)?;
        let mut h = Header {
            compression,
            text_records,
            encryption,
            codepage: 1252,
            version: 0,
            title: None,
            first_image: NONE,
            huff: (0, 0),
            extra_flags: 0,
            ncx: NONE,
            fdst: (NONE, 0),
            frag: NONE,
            skel: NONE,
            exth: Vec::new(),
        };
        if r0.get(16..20) != Some(b"MOBI") {
            return Ok(h);
        }
        let hl = be32(r0, 20).unwrap_or(0) as usize;
        let end = 16 + hl;
        let get = |at: usize| if at + 4 <= end { be32(r0, at) } else { None };
        h.codepage = get(0x1C).unwrap_or(1252);
        h.version = get(0x24).unwrap_or(0);
        if let (Some(off), Some(len)) = (get(0x54), get(0x58)) {
            h.title = r0.get(off as usize..(off + len) as usize).map(<[u8]>::to_vec);
        }
        h.first_image = get(0x6C).unwrap_or(NONE);
        h.huff = (get(0x70).unwrap_or(0), get(0x74).unwrap_or(0));
        if end >= 0xF4 {
            h.extra_flags = be16(r0, 0xF2).unwrap_or(0);
        }
        h.ncx = get(0xF4).unwrap_or(NONE);
        if h.version >= 8 {
            h.fdst = (get(0xC0).unwrap_or(NONE), get(0xC4).unwrap_or(0));
            h.frag = get(0xF8).unwrap_or(NONE);
            h.skel = get(0xFC).unwrap_or(NONE);
        }
        if get(0x80).unwrap_or(0) & 0x40 != 0 && r0.get(end..end + 4) == Some(b"EXTH") {
            let count = be32(r0, end + 8).unwrap_or(0);
            let mut at = end + 12;
            for _ in 0..count {
                let (Some(kind), Some(len)) = (be32(r0, at), be32(r0, at + 4)) else { break };
                let len = len as usize;
                if len < 8 {
                    break;
                }
                if let Some(v) = r0.get(at + 8..at + len) {
                    h.exth.push((kind, v.to_vec()));
                }
                at += len;
            }
        }
        Ok(h)
    }

    pub fn exth(&self, kind: u32) -> impl Iterator<Item = &[u8]> {
        self.exth.iter().filter(move |(k, _)| *k == kind).map(|(_, v)| v.as_slice())
    }

    pub fn exth_u32(&self, kind: u32) -> Option<u32> {
        self.exth(kind).next().and_then(|v| be32(v, 0))
    }

    pub fn decode(&self, b: &[u8]) -> String {
        match self.codepage {
            1252 => encoding_rs::WINDOWS_1252.decode(b).0.into_owned(),
            _ => String::from_utf8_lossy(b).into_owned(),
        }
    }
}

/// 正文记录末尾附带的数据有多长（extra_flags 说明有哪些）
fn trailing_size(data: &[u8], flags: u16) -> usize {
    let mut num = 0usize;
    let mut f = flags >> 1;
    while f != 0 {
        if f & 1 != 0 {
            // 从末尾往前读的变长整数
            let mut size = data.len().saturating_sub(num);
            let (mut v, mut shift) = (0usize, 0);
            while size > 0 {
                let b = data[size - 1];
                v |= usize::from(b & 0x7F) << shift;
                shift += 7;
                size -= 1;
                if b & 0x80 != 0 || shift >= 28 {
                    break;
                }
            }
            num += v;
        }
        f >>= 1;
    }
    if flags & 1 != 0 {
        if let Some(&b) = data.len().checked_sub(num + 1).and_then(|i| data.get(i)) {
            num += usize::from(b & 3) + 1;
        }
    }
    num.min(data.len())
}

/// 解出这一节的全部正文字节
pub(crate) fn text_bytes(records: &[&[u8]], base: usize, h: &Header) -> Result<Vec<u8>> {
    let mut huff = if h.compression == 17480 {
        let at = base + h.huff.0 as usize;
        let huff = *records.get(at).ok_or_else(corrupt_error)?;
        let cdics: Vec<&[u8]> = (1..h.huff.1 as usize).filter_map(|k| records.get(at + k).copied()).collect();
        Some(huffcdic::HuffCdic::new(huff, &cdics)?)
    } else {
        None
    };
    let mut out = Vec::new();
    for i in 1..=h.text_records {
        let Some(&rec) = records.get(base + i) else { break };
        let rec = &rec[..rec.len() - trailing_size(rec, h.extra_flags)];
        match (h.compression, huff.as_mut()) {
            (1, _) => out.extend_from_slice(rec),
            (2, _) => out.extend_from_slice(&decompress_palmdoc(rec)),
            (17480, Some(hc)) => out.extend_from_slice(&hc.unpack(rec)?),
            _ => return Err(corrupt_error()),
        }
    }
    Ok(out)
}

/// 按文件头的几个字节认图片类型
pub(crate) fn image_kind(data: &[u8]) -> Option<(&'static str, &'static str)> {
    if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(("image/jpeg", "jpg"))
    } else if data.starts_with(b"\x89PNG") {
        Some(("image/png", "png"))
    } else if data.starts_with(b"GIF8") {
        Some(("image/gif", "gif"))
    } else if data.starts_with(b"BM") {
        Some(("image/bmp", "bmp"))
    } else {
        None
    }
}

/// 收集用到的图片：第 n 张（从 1 起）= 记录 base + first_image + n - 1
pub(crate) struct Images<'a> {
    records: &'a [&'a [u8]],
    first: Option<usize>,
    pub out: Vec<SourceImage>,
}

impl<'a> Images<'a> {
    pub fn new(records: &'a [&'a [u8]], base: usize, first_image: u32) -> Self {
        let first = (first_image != NONE).then(|| base + first_image as usize);
        Images { records, first, out: Vec::new() }
    }

    pub fn name(&mut self, n: u32) -> Option<String> {
        let idx = self.first? + (n as usize).checked_sub(1)?;
        let data = *self.records.get(idx)?;
        let (mime, ext) = image_kind(data)?;
        let name = format!("images/{n:05}.{ext}");
        if !self.out.iter().any(|i| i.name == name) {
            self.out.push(SourceImage { name: name.clone(), mime: mime.into(), data: data.to_vec() });
        }
        Some(name)
    }
}

fn meta(db: &palmdb::PalmDb, h: &Header) -> (Option<String>, Option<String>) {
    let clean = |s: String| Some(s.trim().to_string()).filter(|s| !s.is_empty());
    let title = h
        .exth(503)
        .next()
        .map(|b| h.decode(b))
        .and_then(clean)
        .or_else(|| h.title.as_deref().map(|b| h.decode(b)).and_then(clean))
        .or_else(|| clean(String::from_utf8_lossy(db.name).replace('_', " ")));
    let authors: Vec<String> = h.exth(100).map(|b| h.decode(b)).filter_map(clean).collect();
    (title, (!authors.is_empty()).then(|| authors.join(" & ")))
}

/// 读目录索引：（正文位置或 KF8 的 pos_fid，标题，层级）
pub(crate) fn read_ncx(records: &[&[u8]], base: usize, h: &Header) -> Vec<indx::Entry> {
    if h.ncx == NONE {
        return Vec::new();
    }
    let decode = |b: &[u8]| h.decode(b);
    match indx::read(records, base + h.ncx as usize, &decode) {
        Ok((mut entries, cncx)) => {
            // 标题在字符串表里，借 ident 字段带出去
            for e in &mut entries {
                if let Some(label) = e.tag(3, 0).and_then(|off| cncx.get(&off)) {
                    e.ident = label.clone();
                }
            }
            entries
        }
        Err(e) => {
            tracing::warn!(error = %e, "mobi 目录索引读不出，改用章节标题");
            Vec::new()
        }
    }
}

fn read_mobi6(db: &palmdb::PalmDb, h: &Header) -> Result<BookSource> {
    let raw = text_bytes(&db.records, 0, h)?;
    let ncx: Vec<mobi6::NcxItem> = read_ncx(&db.records, 0, h)
        .into_iter()
        .filter_map(|e| Some(mobi6::NcxItem { filepos: e.tag(1, 0)? as usize, depth: i64::from(e.tag(4, 0).unwrap_or(0)), label: e.ident }))
        .collect();
    let mut images = Images::new(&db.records, 0, h.first_image);
    let decode = |b: &[u8]| h.decode(b);
    let conv = mobi6::convert(raw, &decode, &ncx, &mut |n| images.name(n));
    if conv.chapters.is_empty() {
        return Err(Error::Other(crate::i18n::t("这本书里没有可读的内容", "This book has no readable content").into()));
    }
    let cover = h.exth_u32(201).and_then(|off| images.name(off + 1));
    let (title, author) = meta(db, h);
    Ok(BookSource { title, author, chapters: conv.chapters, images: images.out, toc: conv.toc, cover })
}

/// 纯 PalmDOC 文本（TEXtREAd）：没有排版，按空行分段
fn read_palmdoc(db: &palmdb::PalmDb, h: &Header) -> Result<BookSource> {
    let raw = text_bytes(&db.records, 0, h)?;
    let text = crate::db::articles::decode_file(&raw);
    let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let body: String = text.split("\n\n").map(str::trim).filter(|p| !p.is_empty()).map(|p| format!("<p>{}</p>", esc(p))).collect();
    let title = Some(String::from_utf8_lossy(db.name).replace('_', " ")).filter(|t| !t.trim().is_empty());
    Ok(BookSource { title, chapters: vec![crate::epub_write::SourceChapter { title: mobi6::section_title(0), body }], ..Default::default() })
}

pub fn read_bytes(bytes: &[u8]) -> Result<BookSource> {
    let db = palmdb::parse(bytes)?;
    let h = Header::parse(db.records[0])?;
    if h.encryption != 0 {
        return Err(drm_error());
    }
    match &db.kind {
        b"TEXtREAd" => read_palmdoc(&db, &h),
        b"BOOKMOBI" => read_mobi6(&db, &h),
        _ => Err(corrupt_error()),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn fixture(name: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mobi").join(name)
    }

    /// 拼一个最小的 MOBI6：一条不压缩的正文记录
    pub fn tiny_mobi(text: &[u8], codepage: u32, title: &str) -> Vec<u8> {
        let mut r0 = Vec::new();
        r0.extend_from_slice(&1u16.to_be_bytes()); // 不压缩
        r0.extend_from_slice(&0u16.to_be_bytes());
        r0.extend_from_slice(&(text.len() as u32).to_be_bytes());
        r0.extend_from_slice(&1u16.to_be_bytes()); // 正文记录数
        r0.extend_from_slice(&4096u16.to_be_bytes());
        r0.extend_from_slice(&0u16.to_be_bytes()); // 加密
        r0.extend_from_slice(&0u16.to_be_bytes());
        let header_len = 0xE8u32;
        let mut m = vec![0u8; header_len as usize];
        m[0..4].copy_from_slice(b"MOBI");
        m[4..8].copy_from_slice(&header_len.to_be_bytes());
        m[8..12].copy_from_slice(&2u32.to_be_bytes());
        m[12..16].copy_from_slice(&codepage.to_be_bytes());
        m[20..24].copy_from_slice(&6u32.to_be_bytes());
        // 相对 record0：0x50 首个非正文记录、0x54/0x58 书名、0x6C 首张图片、0xF4 NCX 索引
        let put = |m: &mut Vec<u8>, at: usize, v: u32| m[at - 16..at - 12].copy_from_slice(&v.to_be_bytes());
        put(&mut m, 0x50, 2);
        let name_off = 16 + header_len;
        put(&mut m, 0x54, name_off);
        put(&mut m, 0x58, title.len() as u32);
        put(&mut m, 0x6C, u32::MAX);
        put(&mut m, 0xF4, u32::MAX);
        r0.extend_from_slice(&m);
        r0.extend_from_slice(title.as_bytes());
        r0.extend_from_slice(&[0, 0]);
        palmdb::tests::build(b"BOOKMOBI", &[r0, text.to_vec()])
    }

    #[test]
    fn reads_old_mobi_fixture() -> Result<()> {
        let book = read(&fixture("alice-old.mobi"))?;
        assert_eq!(book.title.as_deref(), Some("Alice's Adventures in Wonderland"));
        assert_eq!(book.author.as_deref(), Some("Lewis Carroll"));
        assert!(book.chapters.len() >= 2, "{}", book.chapters.len());
        let all: String = book.chapters.iter().map(|c| c.body.as_str()).collect();
        assert!(all.contains("Down the Rabbit-Hole") && all.contains("The Pool of Tears"));
        assert!(!all.to_ascii_lowercase().contains("<mbp:"), "Kindle 专用标签要去掉");
        assert!(all.contains("Alice was beginning to get very tired"));
        let titles: Vec<&str> = book.toc.iter().map(|t| t.title.as_str()).collect();
        assert!(titles.iter().any(|t| t.contains("Rabbit-Hole")) && titles.iter().any(|t| t.contains("Pool of Tears")), "{titles:?}");
        // 正文里的图片指向写出去的图片文件
        assert!(!book.images.is_empty());
        for img in &book.images {
            assert!(img.name.starts_with("images/") && !img.data.is_empty() && img.mime.starts_with("image/"));
        }
        assert!(all.contains("../images/"), "img 的 recindex 换成文件路径");
        Ok(())
    }

    #[test]
    fn rejects_drm() -> Result<()> {
        let mut bytes = std::fs::read(fixture("alice-old.mobi"))?;
        let r0 = u32::from_be_bytes(bytes[78..82].try_into().unwrap()) as usize;
        bytes[r0 + 12..r0 + 14].copy_from_slice(&2u16.to_be_bytes());
        let e = read_bytes(&bytes).unwrap_err().to_string();
        assert!(e.contains("DRM"), "{e}");
        Ok(())
    }

    /// 手动验收：`LENS_MOBI_DIR=<目录> cargo test --lib reads_library -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn reads_library() {
        let dir = std::env::var("LENS_MOBI_DIR").expect("LENS_MOBI_DIR");
        let mut stack = vec![std::path::PathBuf::from(dir)];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| ["mobi", "azw3", "azw"].iter().any(|y| x.eq_ignore_ascii_case(y))) {
                    match read(&p) {
                        Ok(b) => {
                            let words: usize = b.chapters.iter().map(|c| mobi6::plain(&c.body).split_whitespace().count()).sum();
                            println!("OK  {:>4} 章 {:>3} 目录 {:>3} 图 {:>7} 词  {}", b.chapters.len(), b.toc.len(), b.images.len(), words, b.title.unwrap_or_default());
                        }
                        Err(err) => println!("ERR {err}  {}", p.display()),
                    }
                }
            }
        }
    }

    #[test]
    fn rejects_junk() {
        assert!(read_bytes(b"definitely not a palm database").is_err());
    }

    #[test]
    fn decodes_cp1252_and_utf8() -> Result<()> {
        let body = |b: &BookSource| b.chapters.iter().map(|c| c.body.clone()).collect::<String>();
        let latin = encoding_rs::WINDOWS_1252.encode("<html><body><p>Caf\u{e9} \u{201c}ol\u{e9}\u{201d}</p></body></html>").0.into_owned();
        let b = read_bytes(&tiny_mobi(&latin, 1252, "Caf\u{e9}"))?;
        assert!(body(&b).contains("Caf\u{e9} \u{201c}ol\u{e9}\u{201d}"), "{}", body(&b));
        let utf = "<html><body><p>中文 caf\u{e9}</p></body></html>".as_bytes().to_vec();
        let b = read_bytes(&tiny_mobi(&utf, 65001, "冯唐"))?;
        assert!(body(&b).contains("中文 caf\u{e9}"));
        assert_eq!(b.title.as_deref(), Some("冯唐"));
        Ok(())
    }
}
