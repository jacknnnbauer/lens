//! Merriam-Webster's Collegiate® Dictionary API（dictionaryapi.com，需用户自己的 Key，限非商业用途）。
//! GET https://www.dictionaryapi.com/api/v3/references/collegiate/json/<word>?key=<key>
//! 命中返回词条对象数组；未收录返回拼写建议（字符串数组）或空数组。

use async_trait::async_trait;
use serde_json::Value;
use url::Url;

use super::{esc, get_json, sound_link, Ctx, KeyKind, OnlineMode, OnlineProvider, STYLE};
use crate::dict::{EntryBody, RawEntry};
use crate::error::{Error, Result};

pub const ID: &str = "mw";
const BASE: &str = "https://www.dictionaryapi.com/api/v3/references/collegiate/json/";

pub struct MerriamWebster;

#[async_trait]
impl OnlineProvider for MerriamWebster {
    fn id(&self) -> &'static str {
        ID
    }

    fn mode(&self) -> OnlineMode {
        OnlineMode::Api
    }

    fn key_kind(&self) -> KeyKind {
        KeyKind::Key
    }

    async fn lookup(&self, ctx: &Ctx<'_>, word: &str) -> Result<Vec<RawEntry>> {
        let key = ctx.cred.map(|c| c.key.as_str()).ok_or_else(|| Error::Online("缺少 API Key".into()))?;
        let mut url = Url::parse(BASE).map_err(|e| Error::Online(e.to_string()))?;
        url.path_segments_mut().map_err(|_| Error::Online("地址无效".into()))?.pop_if_empty().push(word.trim());
        url.query_pairs_mut().append_pair("key", key);
        let json = get_json(ctx.client.get(url)).await?;
        // Key 无效时接口返回 200 + 纯文本「Invalid API key」，get_json 会报解析失败
        Ok(json.map(|v| render(&v, word)).unwrap_or_default())
    }

    fn page_url(&self, word: &str) -> Option<Url> {
        let mut url = Url::parse("https://www.merriam-webster.com/dictionary/").ok()?;
        url.path_segments_mut().ok()?.pop_if_empty().push(word.trim());
        Some(url)
    }

    fn media_hosts(&self) -> &'static [&'static str] {
        &["media.merriam-webster.com"]
    }
}

/// 发音音频地址（接口文档规定的目录规则）。
fn audio_url(audio: &str) -> Option<String> {
    let first = audio.chars().next()?;
    let subdir = if audio.starts_with("bix") {
        "bix".to_string()
    } else if audio.starts_with("gg") {
        "gg".to_string()
    } else if first.is_ascii_digit() || first.is_ascii_punctuation() {
        "number".to_string()
    } else {
        first.to_ascii_lowercase().to_string()
    };
    Some(format!("https://media.merriam-webster.com/audio/prons/en/us/mp3/{subdir}/{audio}.mp3"))
}

fn entry_link(word: &str) -> String {
    let w = word.split(':').next().unwrap_or(word);
    format!("<a href=\"entry://{}\">{}</a>", esc(w), esc(w))
}

/// 韦氏正文里的 `{…}` 标记 → HTML；未知标记丢弃，其余文字转义。
pub fn markup(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        out.push_str(&esc(&rest[..start]));
        let Some(len) = rest[start..].find('}') else {
            out.push_str(&esc(&rest[start..]));
            return out;
        };
        let token = &rest[start + 1..start + len];
        rest = &rest[start + len + 1..];
        let mut parts = token.split('|');
        let name = parts.next().unwrap_or_default();
        let arg = parts.next().unwrap_or_default();
        out.push_str(&match name {
            "bc" => "<b>:</b> ".to_string(),
            "b" | "parahw" => "<b>".into(),
            "/b" | "/parahw" => "</b>".into(),
            "it" | "wi" | "qword" => "<i>".into(),
            "/it" | "/wi" | "/qword" => "</i>".into(),
            "phrase" => "<b><i>".into(),
            "/phrase" => "</i></b>".into(),
            "inf" => "<sub>".into(),
            "/inf" => "</sub>".into(),
            "sup" => "<sup>".into(),
            "/sup" => "</sup>".into(),
            "sc" => "<span style=\"font-variant:small-caps\">".into(),
            "/sc" => "</span>".into(),
            "ldquo" => "“".into(),
            "rdquo" => "”".into(),
            "gloss" => "[".into(),
            "/gloss" => "]".into(),
            "dx" => " — ".into(),
            "dx_def" | "dx_ety" => "(".into(),
            "/dx_def" | "/dx_ety" => ")".into(),
            "sx" => format!("<span style=\"font-variant:small-caps\">{}</span>", entry_link(arg)),
            "a_link" | "d_link" | "i_link" | "et_link" | "mat" | "dxt" => entry_link(arg),
            _ => String::new(),
        });
    }
    out.push_str(&esc(rest));
    out
}

/// 一个义项的 `dt`（释义文本、例句、用法说明）。
fn defining_text(dt: &[Value], html: &mut String) {
    for item in dt {
        let Some(pair) = item.as_array() else { continue };
        match (pair.first().and_then(Value::as_str), pair.get(1)) {
            (Some("text"), Some(Value::String(t))) => html.push_str(&markup(t)),
            (Some("vis"), Some(Value::Array(vis))) => {
                for v in vis {
                    if let Some(t) = v.get("t").and_then(Value::as_str) {
                        html.push_str(&format!("<span class=\"x\">{}</span>", markup(t)));
                    }
                }
            }
            (Some("uns"), Some(Value::Array(notes))) => {
                for note in notes {
                    if let Some(inner) = note.as_array() {
                        html.push_str(" — ");
                        defining_text(inner, html);
                    }
                }
            }
            _ => {}
        }
    }
}

fn sense(obj: &Value, html: &mut String) {
    html.push_str("<li class=\"sense\">");
    if let Some(sn) = obj.get("sn").and_then(Value::as_str) {
        html.push_str(&format!("<span class=\"num\">{}</span> ", esc(sn)));
    }
    if let Some(dt) = obj.get("dt").and_then(Value::as_array) {
        defining_text(dt, html);
    }
    if let Some(sd) = obj.get("sdsense") {
        if let Some(word) = sd.get("sd").and_then(Value::as_str) {
            html.push_str(&format!(" <i>{}</i> ", esc(word)));
        }
        if let Some(dt) = sd.get("dt").and_then(Value::as_array) {
            defining_text(dt, html);
        }
    }
    html.push_str("</li>");
}

/// `sseq`：义项序列 → 有序列表；`pseq` 为带括号的子序列，`bs` 为总括义项。
fn sense_sequence(sseq: &[Value], html: &mut String) {
    html.push_str("<ol>");
    for group in sseq {
        for item in group.as_array().map(Vec::as_slice).unwrap_or_default() {
            let Some(pair) = item.as_array() else { continue };
            match (pair.first().and_then(Value::as_str), pair.get(1)) {
                (Some("sense"), Some(obj)) => sense(obj, html),
                (Some("bs"), Some(obj)) => {
                    if let Some(s) = obj.get("sense") {
                        sense(s, html);
                    }
                }
                (Some("pseq"), Some(Value::Array(inner))) => {
                    for p in inner {
                        if let Some([kind, obj]) = p.as_array().map(Vec::as_slice) {
                            if kind.as_str() == Some("sense") {
                                sense(obj, html);
                            } else if let Some(s) = obj.get("sense") {
                                sense(s, html);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    html.push_str("</ol>");
}

fn matches_word(entry: &Value, word: &str) -> bool {
    let meta = entry.get("meta");
    let id = meta.and_then(|m| m.get("id")).and_then(Value::as_str).unwrap_or_default();
    let base = id.split(':').next().unwrap_or_default();
    base.eq_ignore_ascii_case(word)
        || meta
            .and_then(|m| m.get("stems"))
            .and_then(Value::as_array)
            .is_some_and(|stems| stems.iter().filter_map(Value::as_str).any(|s| s.eq_ignore_ascii_case(word)))
}

pub fn render(json: &Value, word: &str) -> Vec<RawEntry> {
    let entries: Vec<&Value> = json.as_array().map(|a| a.iter().filter(|e| e.is_object()).collect()).unwrap_or_default();
    // 接口会顺带返回相关词条（复合词、派生词），只留词头或屈折形式与查询词一致的
    let mut picked: Vec<&Value> = entries.iter().copied().filter(|e| matches_word(e, word.trim())).collect();
    if picked.is_empty() {
        picked = entries.into_iter().take(1).collect();
    }
    let count = picked.len();
    picked
        .into_iter()
        .enumerate()
        .map(|(i, e)| {
            let hwi = e.get("hwi");
            let hw = hwi.and_then(|h| h.get("hw")).and_then(Value::as_str).unwrap_or(word).replace('*', "·");
            let headword = hw.replace('·', "");
            let mut html = format!("{STYLE}<div class=\"lens-online\"><div class=\"hwline\"><span class=\"hw\">{}</span>", esc(&hw));
            if let Some(hom) = e.get("hom").and_then(Value::as_u64) {
                html.push_str(&format!("<sup>{hom}</sup>"));
            }
            if let Some(pr) = hwi.and_then(|h| h.get("prs")).and_then(Value::as_array).and_then(|p| p.first()) {
                if let Some(mw) = pr.get("mw").and_then(Value::as_str) {
                    html.push_str(&format!("<span class=\"phon\">\\{}\\</span>", esc(mw)));
                }
                if let Some(url) = pr.get("sound").and_then(|s| s.get("audio")).and_then(Value::as_str).and_then(audio_url) {
                    html.push_str(&sound_link(&url));
                }
            }
            html.push_str("</div>");
            if let Some(fl) = e.get("fl").and_then(Value::as_str) {
                html.push_str(&format!("<div class=\"pos\">{}</div>", esc(fl)));
            }
            for d in e.get("def").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default() {
                if let Some(vd) = d.get("vd").and_then(Value::as_str) {
                    html.push_str(&format!("<div class=\"pos\">{}</div>", esc(vd)));
                }
                if let Some(sseq) = d.get("sseq").and_then(Value::as_array) {
                    sense_sequence(sseq, &mut html);
                }
            }
            if i + 1 == count {
                let page = MerriamWebster.page_url(&headword).map(|u| u.to_string()).unwrap_or_default();
                html.push_str(&format!(
                    "<p class=\"source\">来源：<a href=\"{}\">Merriam-Webster’s Collegiate® Dictionary</a></p>",
                    esc(&page)
                ));
            }
            html.push_str("</div>");
            RawEntry { headword, body: EntryBody::Html(html) }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markup_tokens() {
        assert_eq!(markup("{bc}having great {a_link|volume}"), "<b>:</b> having great <a href=\"entry://volume\">volume</a>");
        assert_eq!(markup("long {wi}voluminous{/wi} tresses"), "long <i>voluminous</i> tresses");
        assert_eq!(markup("{sx|large||}"), "<span style=\"font-variant:small-caps\"><a href=\"entry://large\">large</a></span>");
        assert_eq!(markup("a < b {unknown} {ldquo}x{rdquo}"), "a &lt; b  “x”");
        assert_eq!(markup("broken {tag"), "broken {tag");
    }

    #[test]
    fn audio_subdirectories() {
        assert_eq!(audio_url("volumi02").as_deref(), Some("https://media.merriam-webster.com/audio/prons/en/us/mp3/v/volumi02.mp3"));
        assert!(audio_url("bixa0001").is_some_and(|u| u.contains("/bix/")));
        assert!(audio_url("ggabc").is_some_and(|u| u.contains("/gg/")));
        assert!(audio_url("3d00001").is_some_and(|u| u.contains("/number/")));
        assert!(audio_url("").is_none());
    }

    #[test]
    fn renders_matching_entries_only() {
        let v: Value = serde_json::from_str(include_str!("../../../fixtures/online/mw-voluminous.json")).unwrap_or(Value::Null);
        let entries = render(&v, "voluminous");
        assert_eq!(entries.len(), 1, "相关词条 voluminousness 不应单独出现");
        assert_eq!(entries[0].headword, "voluminous");
        let EntryBody::Html(html) = &entries[0].body else { panic!() };
        assert!(html.contains("vo·lu·mi·nous"));
        assert!(html.contains("sound://remote/media.merriam-webster.com/audio/prons/en/us/mp3/v/volumi02.mp3"));
        assert!(html.contains("<span class=\"num\">1 a</span>"));
        assert!(html.contains("<span class=\"x\">long <i>voluminous</i> tresses</span>"));
        assert!(html.contains("<i>also</i>"));
        assert!(html.contains("Merriam-Webster’s Collegiate® Dictionary"));
    }

    #[test]
    fn suggestions_mean_miss() {
        assert!(render(&serde_json::json!(["volume", "voluminous"]), "volumnous").is_empty());
        assert!(render(&serde_json::json!([]), "x").is_empty());
    }
}
