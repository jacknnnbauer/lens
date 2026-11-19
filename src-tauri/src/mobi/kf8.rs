//! KF8（.azw3，以及 Calibre 常见的「MOBI6 + KF8」合体 .mobi）。
//! 正文按「骨架 + 片段」存放：先用 SKEL / FRAG 索引还原出每个 XHTML 文件，再把 `kindle:` 链接和图片地址换成普通路径。

use std::collections::HashMap;

use super::mobi6::{attr, body_of, first_heading, plain, rewrite_tags, section_title};
use super::{indx, read_ncx, text_bytes, Header, Images, NONE};
use crate::epub_write::{BookSource, SourceChapter, SourceToc};
use crate::error::Result;

/// 一个还原出来的文件：内容，以及它在正文流里的起止位置
struct Part {
    bytes: Vec<u8>,
    start: usize,
    end: usize,
}

/// Kindle 的 base32（0-9A-V）
fn base32(s: &str) -> Option<usize> {
    let mut v = 0usize;
    for c in s.chars() {
        let d = c.to_digit(32)? as usize;
        v = v.checked_mul(32)?.checked_add(d)?;
    }
    Some(v)
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if from >= hay.len() || hay.len() - from < needle.len() {
        return None;
    }
    hay[from..].windows(needle.len()).position(|w| w == needle).map(|p| p + from)
}

fn rfind(hay: &[u8], byte: u8) -> Option<usize> {
    hay.iter().rposition(|&b| b == byte)
}

/// 位置 `pos`（在这个文件里）之前最近的 id / name
fn anchor_before(part: &[u8], pos: usize) -> Option<String> {
    let pos = pos.min(part.len());
    // 落在标签里面：算到这个标签结束
    let gt = find(part, b">", pos);
    let lt = find(part, b"<", pos);
    let cut = match (gt, lt) {
        (Some(g), Some(l)) if l == pos || g < l => g + 1,
        (Some(g), None) => g + 1,
        _ => pos,
    };
    let block = String::from_utf8_lossy(&part[..cut]);
    let mut best: Option<(usize, String)> = None;
    let mut from = 0;
    while let Some(i) = block[from..].find('<').map(|i| from + i) {
        let end = block[i..].find('>').map_or(block.len(), |e| i + e + 1);
        let tag = &block[i..end];
        if let Some((v, _)) = attr(tag, "id").or_else(|| attr(tag, "name")) {
            if !v.is_empty() {
                best = Some((i, v));
            }
        }
        from = i + 1;
    }
    best.map(|b| b.1)
}

/// `mobi6_first_image`：合体文件里 MOBI6 头的首张图片记录号（图片放在那一半）
pub fn read(records: &[&[u8]], base: usize, h: &Header, db_name: &[u8], mobi6_first_image: Option<u32>) -> Result<BookSource> {
    let bad = super::corrupt_error;
    let raw = text_bytes(records, base, h)?;
    let decode = |b: &[u8]| h.decode(b);

    // 正文流：第一段是 XHTML，后面是 CSS 等（用不上）
    let mut text_end = raw.len();
    if h.fdst.0 != NONE && h.fdst.1 > 1 {
        let f = *records.get(base + h.fdst.0 as usize).ok_or_else(bad)?;
        if f.starts_with(b"FDST") && f.len() >= 20 {
            text_end = (u32::from_be_bytes(f[16..20].try_into().unwrap()) as usize).min(raw.len());
        }
    }
    let text = &raw[..text_end];

    if h.skel == NONE {
        return Err(bad());
    }
    let (skels, _) = indx::read(records, base + h.skel as usize, &decode)?;
    let frags = if h.frag != NONE { indx::read(records, base + h.frag as usize, &decode)?.0 } else { Vec::new() };
    // 片段：插入位置、长度
    let frags: Vec<(usize, usize)> = frags.iter().map(|e| (e.ident.trim().parse::<usize>().unwrap_or(0), e.tag(6, 1).unwrap_or(0) as usize)).collect();

    let mut parts: Vec<Part> = Vec::new();
    let mut fi = 0usize;
    for s in &skels {
        let count = s.tag(1, 0).unwrap_or(0) as usize;
        let skel_pos = s.tag(6, 0).ok_or_else(bad)? as usize;
        let skel_len = s.tag(6, 1).ok_or_else(bad)? as usize;
        let mut base_ptr = skel_pos + skel_len;
        let mut skeleton = text.get(skel_pos..base_ptr).ok_or_else(bad)?.to_vec();
        for _ in 0..count {
            let Some(&(insert, len)) = frags.get(fi) else { break };
            fi += 1;
            let piece = text.get(base_ptr..base_ptr + len).ok_or_else(bad)?;
            let mut at = insert.saturating_sub(skel_pos).min(skeleton.len());
            // 插入点落在标签中间（个别书这样）：挪到标签后面
            let head = &skeleton[..at];
            if rfind(head, b'<') > rfind(head, b'>') {
                at = find(&skeleton, b">", at).map_or(skeleton.len(), |e| e + 1);
            }
            skeleton.splice(at..at, piece.iter().copied());
            base_ptr += len;
        }
        parts.push(Part { bytes: skeleton, start: skel_pos, end: base_ptr });
    }
    if parts.is_empty() {
        return Err(bad());
    }

    // kindle:pos:fid:XXXX:off:YYYYYYYYYY → （第几个文件，锚点）
    let locate = |fid: usize, off: usize| -> Option<(usize, Option<String>)> {
        let (insert, _) = *frags.get(fid)?;
        let pos = insert + off;
        let pi = parts.iter().position(|p| pos >= p.start && pos < p.end).or_else(|| parts.iter().rposition(|p| p.start <= pos))?;
        Some((pi, anchor_before(&parts[pi].bytes, pos - parts[pi].start)))
    };
    let parse_pos = |s: &str| -> Option<(usize, usize)> {
        let rest = s.strip_prefix("kindle:pos:fid:")?;
        let (fid, off) = rest.split_once(":off:")?;
        Some((base32(fid)?, base32(off.get(..10).unwrap_or(off))?))
    };

    // 还原的文件 → 章节（空文件丢掉）；记下文件号到章节号
    let mut images = Images::new(records, base, h.first_image).with_fallback(mobi6_first_image);
    let mut chapter_of: HashMap<usize, usize> = HashMap::new();
    let mut bodies: Vec<(usize, String)> = Vec::new();
    for (pi, p) in parts.iter().enumerate() {
        let html = decode(&p.bytes);
        let body = body_of(&html).to_string();
        // 图片：src="kindle:embed:XXXX?mime=..."
        let body = rewrite_tags(&body, "img", |tag| {
            let (src, _) = attr(tag, "src")?;
            let code = src.strip_prefix("kindle:embed:")?;
            let n = base32(code.split('?').next().unwrap_or(""))?;
            Some(match images.name(n as u32) {
                Some(name) => format!("<img src=\"../{name}\" alt=\"\"/>"),
                None => String::new(),
            })
        });
        if plain(&body).is_empty() && !body.contains("<img") {
            continue;
        }
        chapter_of.insert(pi, bodies.len());
        bodies.push((pi, body));
    }
    if bodies.is_empty() {
        return Err(crate::error::Error::Other(crate::i18n::t("这本书里没有可读的内容", "This book has no readable content").into()));
    }

    let href_for = |pi: usize, anchor: Option<String>| -> Option<String> {
        let c = *chapter_of.get(&pi)?;
        Some(match anchor {
            Some(a) => format!("c{:04}.xhtml#{a}", c + 1),
            None => format!("c{:04}.xhtml", c + 1),
        })
    };
    let chapters: Vec<SourceChapter> = bodies
        .into_iter()
        .enumerate()
        .map(|(ci, (_, body))| {
            let body = rewrite_tags(&body, "a", |tag| {
                let (href, range) = attr(tag, "href")?;
                let target = parse_pos(&href).and_then(|(fid, off)| locate(fid, off)).and_then(|(pi, a)| href_for(pi, a));
                if !href.starts_with("kindle:") && target.is_none() {
                    return None;
                }
                let new = target.unwrap_or_else(|| "#".into());
                Some(format!("{}href=\"{new}\"{}", &tag[..range.start], &tag[range.end..]))
            });
            let title = first_heading(&body).unwrap_or_else(|| section_title(ci));
            SourceChapter { title, body }
        })
        .collect();

    // 目录：标签 6 是 pos_fid（片段号、偏移）
    let toc: Vec<SourceToc> = read_ncx(records, base, h)
        .into_iter()
        .filter_map(|e| {
            let (pi, anchor) = locate(e.tag(6, 0)? as usize, e.tag(6, 1)? as usize)?;
            let chapter = *chapter_of.get(&pi)?;
            let title = e.ident.trim().to_string();
            (!title.is_empty()).then(|| SourceToc { title, level: i64::from(e.tag(4, 0).unwrap_or(0)), chapter, anchor })
        })
        .collect();

    let cover = h.exth_u32(201).and_then(|off| images.name(off + 1));
    let (title, author) = super::meta_of(db_name, h);
    Ok(BookSource { title, author, chapters, images: images.out, toc, cover })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_kindle_base32() {
        assert_eq!(base32("0001"), Some(1));
        assert_eq!(base32("000V"), Some(31));
        assert_eq!(base32("0010"), Some(32));
        assert_eq!(base32("XYZ"), None);
    }

    #[test]
    fn finds_nearest_id_before_position() {
        let part = br#"<body><h2 id="ch1">One</h2><p>text</p><div id="ch2"><p>more</p></div></body>"#;
        let pos = find(part, b"more", 0).unwrap();
        assert_eq!(anchor_before(part, pos).as_deref(), Some("ch2"));
        assert_eq!(anchor_before(part, 3), None);
    }
}
