//! MOBI6（旧格式）：一整段 HTML，按 `<mbp:pagebreak/>` 分章；链接用 filepos（正文字节位置），图片用 recindex。

use std::collections::{BTreeSet, HashMap};

use crate::epub_write::{SourceChapter, SourceToc};

/// 目录项：正文字节位置、标题、层级
pub struct NcxItem {
    pub filepos: usize,
    pub label: String,
    pub depth: i64,
}

pub struct Converted {
    pub chapters: Vec<SourceChapter>,
    pub toc: Vec<SourceToc>,
}

/// 不区分大小写地找
pub(crate) fn find_ci(hay: &str, needle: &str, from: usize) -> Option<usize> {
    let h = hay.as_bytes();
    let n = needle.as_bytes();
    if n.is_empty() || from >= h.len() || h.len() - from < n.len() {
        return None;
    }
    (from..=h.len().saturating_sub(n.len())).find(|&i| h[i..i + n.len()].eq_ignore_ascii_case(n))
}

fn find_bytes(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || from >= hay.len() || hay.len() - from < needle.len() {
        return None;
    }
    (from..=hay.len().saturating_sub(needle.len())).find(|&i| hay[i..i + needle.len()].eq_ignore_ascii_case(needle))
}

/// 读属性值：`name=value`、`name="value"`、`name='value'`；返回（值，属性整段在 tag 里的范围）
pub(crate) fn attr(tag: &str, name: &str) -> Option<(String, std::ops::Range<usize>)> {
    let mut from = 0;
    while let Some(i) = find_ci(tag, name, from) {
        from = i + 1;
        // 前面要是空白，后面紧跟 =（中间可有空格）
        let before_ok = i > 0 && tag.as_bytes()[i - 1].is_ascii_whitespace();
        let rest = &tag[i + name.len()..];
        let trimmed = rest.trim_start();
        if !before_ok || !trimmed.starts_with('=') {
            continue;
        }
        let after_eq = trimmed[1..].trim_start();
        let start_val = tag.len() - after_eq.len();
        let (value, end) = match after_eq.chars().next() {
            Some(q @ ('"' | '\'')) => {
                let close = after_eq[1..].find(q).map_or(after_eq.len(), |p| p + 1);
                (after_eq[1..close].to_string(), start_val + (close + 1).min(after_eq.len()))
            }
            _ => {
                let len = after_eq.find(|c: char| c.is_ascii_whitespace() || c == '>' || c == '/').unwrap_or(after_eq.len());
                (after_eq[..len].to_string(), start_val + len)
            }
        };
        return Some((value, i..end));
    }
    None
}

/// 逐个改写某种标签：`f` 拿到整个标签文字，返回替换内容（None = 不改）
pub(crate) fn rewrite_tags(html: &str, name: &str, mut f: impl FnMut(&str) -> Option<String>) -> String {
    let open = format!("<{name}");
    let mut out = String::with_capacity(html.len());
    let mut pos = 0;
    while let Some(i) = find_ci(html, &open, pos) {
        let next = html.as_bytes().get(i + open.len()).copied().unwrap_or(b'>');
        let Some(end) = html[i..].find('>').map(|e| i + e + 1) else { break };
        out.push_str(&html[pos..i]);
        let tag = &html[i..end];
        if next.is_ascii_alphanumeric() {
            // 只是名字开头一样（<abbr> 之于 <a>）
            out.push_str(tag);
        } else {
            match f(tag) {
                Some(new) => out.push_str(&new),
                None => out.push_str(tag),
            }
        }
        pos = end;
    }
    out.push_str(&html[pos..]);
    out
}

/// 去掉某种前缀的标签（`<mbp:...>`、`</mbp:...>`），内容保留
fn strip_prefixed_tags(html: &str, prefix: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut pos = 0;
    loop {
        let a = find_ci(html, &format!("<{prefix}"), pos);
        let b = find_ci(html, &format!("</{prefix}"), pos);
        let Some(i) = [a, b].into_iter().flatten().min() else { break };
        let Some(end) = html[i..].find('>').map(|e| i + e + 1) else { break };
        out.push_str(&html[pos..i]);
        pos = end;
    }
    out.push_str(&html[pos..]);
    out
}

pub(crate) fn plain(html: &str) -> String {
    crate::epub::plain_of_html(html).split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 第一个标题的文字
pub(crate) fn first_heading(html: &str) -> Option<String> {
    for h in ["<h1", "<h2", "<h3", "<h4"] {
        if let Some(i) = find_ci(html, h, 0) {
            let close = format!("</{}", &h[1..]);
            let start = html[i..].find('>').map(|e| i + e + 1)?;
            let end = find_ci(html, &close, start).unwrap_or(html.len());
            let t = plain(&html[start..end]);
            if !t.is_empty() {
                return Some(t);
            }
        }
    }
    None
}

pub(crate) fn section_title(i: usize) -> String {
    crate::i18n::t("第 {n} 节", "Section {n}").replace("{n}", &(i + 1).to_string())
}

/// `<body>` 里面的部分（没有就整段）
pub(crate) fn body_of(html: &str) -> &str {
    let start = find_ci(html, "<body", 0).and_then(|i| html[i..].find('>').map(|e| i + e + 1)).unwrap_or(0);
    let end = find_ci(html, "</body", start).unwrap_or(html.len());
    &html[start..end]
}

/// 字节位置落在标签中间时挪到标签后面
fn safe_insert_pos(raw: &[u8], p: usize) -> usize {
    let p = p.min(raw.len());
    let lt = raw[..p].iter().rposition(|&b| b == b'<');
    let gt = raw[..p].iter().rposition(|&b| b == b'>');
    match (lt, gt) {
        (Some(l), g) if g.is_none_or(|g| g < l) => raw[p..].iter().position(|&b| b == b'>').map_or(raw.len(), |e| p + e + 1),
        _ => p,
    }
}

/// `image_name(n)`：第 n 张图（recindex，从 1 起）写出去的文件名；没有这张图返回 None
pub fn convert(mut raw: Vec<u8>, decode: &dyn Fn(&[u8]) -> String, ncx: &[NcxItem], image_name: &mut dyn FnMut(u32) -> Option<String>) -> Converted {
    // 1. 链接和目录指向的位置先插上锚点（按字节位置，必须在解码前）
    let mut targets: BTreeSet<usize> = ncx.iter().map(|n| n.filepos).collect();
    let mut from = 0;
    while let Some(i) = find_bytes(&raw, b"filepos=", from) {
        from = i + 8;
        let digits: String = raw[from..].iter().skip_while(|&&b| b == b'"' || b == b'\'').take_while(|b| b.is_ascii_digit()).map(|&b| b as char).collect();
        if let Ok(n) = digits.parse::<usize>() {
            targets.insert(n);
        }
    }
    for &t in targets.iter().rev() {
        if t > raw.len() {
            continue;
        }
        let p = safe_insert_pos(&raw, t);
        let anchor = format!("<a id=\"fp{t}\"></a>");
        raw.splice(p..p, anchor.bytes());
    }
    let text = decode(&raw);
    let body = body_of(&text);

    // 2. 按分页符分章
    let mut pieces: Vec<String> = Vec::new();
    let mut pos = 0;
    while let Some(i) = find_ci(body, "<mbp:pagebreak", pos) {
        pieces.push(body[pos..i].to_string());
        pos = body[i..].find('>').map_or(body.len(), |e| i + e + 1);
    }
    pieces.push(body[pos..].to_string());
    let pieces: Vec<String> = pieces
        .into_iter()
        .map(|p| strip_prefixed_tags(&p, "mbp:"))
        .filter(|p| !plain(p).is_empty() || find_ci(p, "<img", 0).is_some())
        .collect();

    // 3. 锚点在哪一章
    let mut anchor_chapter: HashMap<String, usize> = HashMap::new();
    for (ci, p) in pieces.iter().enumerate() {
        let mut from = 0;
        while let Some(i) = p[from..].find("id=\"fp").map(|i| from + i) {
            let id: String = p[i + 4..].chars().take_while(|&c| c != '"').collect();
            anchor_chapter.entry(id).or_insert(ci);
            from = i + 6;
        }
    }

    // 4. 改链接和图片
    let chapters: Vec<SourceChapter> = pieces
        .iter()
        .enumerate()
        .map(|(ci, p)| {
            let linked = rewrite_tags(p, "a", |tag| {
                let (value, range) = attr(tag, "filepos")?;
                let n: usize = value.trim_start_matches('0').parse().unwrap_or(0);
                let id = format!("fp{n}");
                let href = match anchor_chapter.get(&id) {
                    Some(&c) => format!("c{:04}.xhtml#{id}", c + 1),
                    None => "#".to_string(),
                };
                Some(format!("{}href=\"{href}\"{}", &tag[..range.start], &tag[range.end..]))
            });
            let body = rewrite_tags(&linked, "img", |tag| {
                let n = ["recindex", "hirecindex", "lorecindex"].iter().find_map(|a| attr(tag, a)).and_then(|(v, _)| v.trim_start_matches('0').parse::<u32>().ok());
                Some(match n.and_then(&mut *image_name) {
                    Some(name) => format!("<img src=\"../{name}\" alt=\"\"/>"),
                    None => String::new(),
                })
            });
            let title = first_heading(&body).unwrap_or_else(|| section_title(ci));
            SourceChapter { title, body }
        })
        .collect();

    // 5. 目录
    let toc = ncx
        .iter()
        .filter_map(|n| {
            let id = format!("fp{}", n.filepos);
            let chapter = *anchor_chapter.get(&id)?;
            let label = n.label.trim();
            (!label.is_empty()).then(|| SourceToc { title: label.to_string(), level: n.depth.max(0), chapter, anchor: Some(id) })
        })
        .collect();
    Converted { chapters, toc }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_shorter_than_needle_finds_nothing() {
        assert_eq!(find_bytes(b"abc", b"filepos=", 0), None);
        assert_eq!(find_ci("abc", "<mbp:pagebreak", 0), None);
        assert_eq!(find_ci("x<IMG", "<img", 0), Some(1));
    }

    #[test]
    fn reads_attributes_in_any_quoting() {
        assert_eq!(attr("<a filepos=0000123 >", "filepos").map(|a| a.0).as_deref(), Some("0000123"));
        assert_eq!(attr("<img alt='x' recindex=\"00002\"/>", "recindex").map(|a| a.0).as_deref(), Some("00002"));
        assert!(attr("<img hirecindex=\"1\">", "recindex").is_none(), "不能把 hirecindex 当成 recindex");
    }

    #[test]
    fn splits_pages_links_and_images() {
        let raw = b"<html><head><guide></guide></head><body><h2>One</h2><p><a filepos=0000099>next</a></p><mbp:pagebreak/><h2>Two</h2><p>x<img recindex=\"00001\"/></p></body></html>".to_vec();
        // 指向 "<h2>Two" 的字节位置
        let target = find_bytes(&raw, b"<h2>Two", 0).unwrap();
        let raw = String::from_utf8(raw).unwrap().replace("0000099", &format!("{target:07}")).into_bytes();
        let mut names = |n: u32| (n == 1).then(|| "images/00001.png".to_string());
        let out = convert(raw, &|b| String::from_utf8_lossy(b).into_owned(), &[NcxItem { filepos: target, label: "Two".into(), depth: 0 }], &mut names);
        assert_eq!(out.chapters.len(), 2);
        assert_eq!(out.chapters[1].title, "Two");
        assert!(out.chapters[0].body.contains(&format!("href=\"c0002.xhtml#fp{target}\"")), "{}", out.chapters[0].body);
        assert!(out.chapters[1].body.contains("src=\"../images/00001.png\""));
        assert!(!out.chapters[0].body.contains("guide"));
        assert_eq!(out.toc.len(), 1);
        assert_eq!(out.toc[0].chapter, 1);
    }
}
