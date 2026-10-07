//! mdx / mdd 头部：UTF-16LE 的 `<Dictionary …/>` 或 `<Library_Data …/>`，只取属性。

use std::collections::HashMap;

use encoding_rs::{Encoding, BIG5, GB18030, UTF_16LE, UTF_8};

use crate::error::{Error, Result};

#[derive(Debug, Clone)]
pub struct Header {
    pub version: f32,
    pub encoding: &'static Encoding,
    pub encrypted: u8,
    pub format_text: bool,
    pub title: String,
    /// StyleSheet：编号 → (开始标签, 结束标签)
    pub stylesheet: HashMap<String, (String, String)>,
}

impl Header {
    pub fn is_v2(&self) -> bool {
        self.version >= 2.0
    }
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

/// 宽松解析 `name="value"` 属性（各生成器的头部并不都是合法 XML）。
pub fn parse_attrs(text: &str) -> HashMap<String, String> {
    let mut attrs = HashMap::new();
    let mut rest = text;
    while let Some(eq) = rest.find("=\"") {
        let name = rest[..eq]
            .rsplit(|c: char| c.is_whitespace() || c == '<')
            .next()
            .unwrap_or_default()
            .to_string();
        let after = &rest[eq + 2..];
        let Some(end) = after.find('"') else { break };
        if !name.is_empty() {
            attrs.insert(name, unescape(&after[..end]));
        }
        rest = &after[end + 1..];
    }
    attrs
}

fn parse_encoding(label: &str, is_mdd: bool) -> Result<&'static Encoding> {
    if is_mdd {
        return Ok(UTF_16LE);
    }
    let l = label.trim().to_ascii_uppercase();
    Ok(match l.as_str() {
        "" | "UTF-8" | "UTF8" => UTF_8,
        "UTF-16" | "UTF16" | "UTF-16LE" => UTF_16LE,
        "GBK" | "GB2312" | "GB18030" => GB18030,
        "BIG5" | "BIG-5" => BIG5,
        other => Encoding::for_label(other.as_bytes())
            .ok_or_else(|| Error::InvalidDict(format!("不支持的编码 {label}")))?,
    })
}

fn parse_stylesheet(s: &str) -> HashMap<String, (String, String)> {
    let lines: Vec<&str> = s.lines().collect();
    lines
        .chunks(3)
        .filter(|c| c.len() == 3)
        .map(|c| (c[0].trim().to_string(), (c[1].to_string(), c[2].to_string())))
        .collect()
}

pub fn parse(bytes: &[u8], is_mdd: bool) -> Result<Header> {
    let (text, _) = UTF_16LE.decode_without_bom_handling(bytes);
    let attrs = parse_attrs(text.trim_end_matches('\0'));
    let get = |k: &str| attrs.get(k).map(String::as_str).unwrap_or_default();

    let version: f32 = get("GeneratedByEngineVersion")
        .trim()
        .parse()
        .map_err(|_| Error::InvalidDict("mdx 头部缺少有效的 GeneratedByEngineVersion".into()))?;
    let encrypted = match get("Encrypted").trim() {
        "" | "No" | "no" => 0,
        "Yes" | "yes" => 1,
        n => n.parse().map_err(|_| Error::InvalidDict(format!("Encrypted={n} 无效")))?,
    };
    Ok(Header {
        version,
        encoding: parse_encoding(get("Encoding"), is_mdd)?,
        encrypted,
        format_text: get("Format").eq_ignore_ascii_case("text"),
        title: get("Title").trim().to_string(),
        stylesheet: parse_stylesheet(get("StyleSheet")),
    })
}

/// 把正文里的 `` `N` `` 标记替换成 StyleSheet 定义的标签（与 readmdict 行为一致）：
/// 标记后的文本包在 (开始, 结束) 标签里；文本以换行结尾时去掉尾部空白后补 `\r\n`。
pub fn apply_stylesheet(text: &str, sheet: &HashMap<String, (String, String)>) -> String {
    if sheet.is_empty() || !text.contains('`') {
        return text.to_string();
    }
    let mut pieces: Vec<(Option<&str>, &str)> = Vec::new();
    let (mut tag, mut seg_start, mut i) = (None, 0, 0);
    let b = text.as_bytes();
    while i < b.len() {
        if b[i] == b'`' {
            let digits = b[i + 1..].iter().take_while(|c| c.is_ascii_digit()).count();
            if digits > 0 && b.get(i + 1 + digits) == Some(&b'`') {
                pieces.push((tag, &text[seg_start..i]));
                tag = Some(&text[i + 1..i + 1 + digits]);
                i += digits + 2;
                seg_start = i;
                continue;
            }
        }
        i += 1;
    }
    pieces.push((tag, &text[seg_start..]));

    let mut out = String::with_capacity(text.len() + 64);
    for (tag, p) in pieces {
        match tag.and_then(|t| sheet.get(t)) {
            Some((open, close)) if p.ends_with('\n') => {
                out.push_str(open);
                out.push_str(p.trim_end());
                out.push_str(close);
                out.push_str("\r\n");
            }
            Some((open, close)) => {
                out.push_str(open);
                out.push_str(p);
                out.push_str(close);
            }
            None => out.push_str(p),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_loose_attributes_with_entities() {
        let a = parse_attrs(r#"<Dictionary GeneratedByEngineVersion="2.0" Title="A &amp; B" Description="&lt;p&gt;x&lt;/p&gt;"/>"#);
        assert_eq!(a["GeneratedByEngineVersion"], "2.0");
        assert_eq!(a["Title"], "A & B");
        assert_eq!(a["Description"], "<p>x</p>");
    }

    #[test]
    fn stylesheet_wraps_segments() {
        let sheet = parse_stylesheet("1\r\n<b>\r\n</b>\r\n2\r\n<i>\r\n</i>");
        assert_eq!(apply_stylesheet("x`1`bold`2`it\r\n", &sheet), "x<b>bold</b><i>it</i>\r\n");
        // 未定义的编号丢掉标记、保留文本；反引号后不是数字时原样保留
        assert_eq!(apply_stylesheet("`9`a `b`", &sheet), "a `b`");
    }

    #[test]
    fn encoding_labels() -> Result<()> {
        assert_eq!(parse_encoding("", false)?, UTF_8);
        assert_eq!(parse_encoding("GBK", false)?, GB18030);
        assert_eq!(parse_encoding("UTF-16", false)?, UTF_16LE);
        assert_eq!(parse_encoding("UTF-8", true)?, UTF_16LE);
        assert!(parse_encoding("KLINGON", false).is_err());
        Ok(())
    }
}
