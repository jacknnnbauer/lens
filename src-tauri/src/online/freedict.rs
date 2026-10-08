//! Free Dictionary API（dictionaryapi.dev）：免 Key，释义来自 Wiktionary（CC BY-SA 3.0）。
//! GET https://api.dictionaryapi.dev/api/v2/entries/en/<word>；未收录返回 404。

use async_trait::async_trait;
use serde_json::Value;
use url::Url;

use super::{esc, get_json, sound_link, Ctx, OnlineMode, OnlineProvider, STYLE};
use crate::dict::{EntryBody, RawEntry};
use crate::error::Result;

pub const ID: &str = "freedict";
const BASE: &str = "https://api.dictionaryapi.dev/api/v2/entries/en/";

pub struct FreeDictionary;

#[async_trait]
impl OnlineProvider for FreeDictionary {
    fn id(&self) -> &'static str {
        ID
    }

    fn mode(&self) -> OnlineMode {
        OnlineMode::Api
    }

    async fn lookup(&self, ctx: &Ctx<'_>, word: &str) -> Result<Vec<RawEntry>> {
        let Some(url) = self.page_url(word) else { return Ok(Vec::new()) };
        Ok(get_json(ctx.client.get(url)).await?.map(|v| render(&v)).unwrap_or_default())
    }

    fn page_url(&self, word: &str) -> Option<Url> {
        let mut url = Url::parse(BASE).ok()?;
        url.path_segments_mut().ok()?.pop_if_empty().push(word.trim());
        Some(url)
    }

    fn media_hosts(&self) -> &'static [&'static str] {
        &["api.dictionaryapi.dev"]
    }
}

fn str_of<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str).filter(|s| !s.trim().is_empty())
}

fn arr<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key).and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default()
}

/// 接口返回一个数组，每项是一个词源（同形异义）；合并成一条词条。
pub fn render(json: &Value) -> Vec<RawEntry> {
    let Some(items) = json.as_array().filter(|a| !a.is_empty()) else { return Vec::new() };
    let mut html = format!("{STYLE}<div class=\"lens-online\">");
    let mut headword = String::new();
    let mut license: Option<(String, String)> = None;
    for item in items {
        let word = str_of(item, "word").unwrap_or_default();
        if headword.is_empty() {
            headword = word.to_string();
        }
        let phonetics = arr(item, "phonetics");
        let ipa = str_of(item, "phonetic").or_else(|| phonetics.iter().find_map(|p| str_of(p, "text")));
        let audio = phonetics.iter().find_map(|p| str_of(p, "audio"));
        html.push_str(&format!("<div class=\"hwline\"><span class=\"hw\">{}</span>", esc(word)));
        if let Some(ipa) = ipa {
            html.push_str(&format!("<span class=\"phon\">{}</span>", esc(ipa)));
        }
        if let Some(a) = audio {
            html.push_str(&sound_link(a));
        }
        html.push_str("</div>");
        for m in arr(item, "meanings") {
            if let Some(pos) = str_of(m, "partOfSpeech") {
                html.push_str(&format!("<div class=\"pos\">{}</div>", esc(pos)));
            }
            html.push_str("<ol>");
            for d in arr(m, "definitions") {
                let Some(def) = str_of(d, "definition") else { continue };
                html.push_str(&format!("<li class=\"sense\">{}", esc(def)));
                if let Some(ex) = str_of(d, "example") {
                    html.push_str(&format!("<span class=\"x\">{}</span>", esc(ex)));
                }
                html.push_str("</li>");
            }
            html.push_str("</ol>");
            let syn: Vec<&str> = arr(m, "synonyms").iter().filter_map(Value::as_str).take(8).collect();
            if !syn.is_empty() {
                html.push_str(&format!("<span class=\"syn\">同义：{}</span>", esc(&syn.join(", "))));
            }
        }
        if license.is_none() {
            if let (Some(name), Some(url)) = (item.get("license").and_then(|l| str_of(l, "name")), item.get("license").and_then(|l| str_of(l, "url"))) {
                license = Some((name.to_string(), url.to_string()));
            }
        }
    }
    let source = arr(&items[0], "sourceUrls").iter().find_map(Value::as_str).unwrap_or("https://en.wiktionary.org/");
    html.push_str(&format!(
        "<p class=\"source\">来源：<a href=\"{}\">Wiktionary</a>，经 Free Dictionary API",
        esc(source)
    ));
    if let Some((name, url)) = license {
        html.push_str(&format!(" · <a href=\"{}\">{}</a>", esc(&url), esc(&name)));
    }
    html.push_str("</p></div>");
    vec![RawEntry { headword, body: EntryBody::Html(html) }]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        serde_json::from_str(include_str!("../../../fixtures/online/freedict-colour.json")).unwrap_or(Value::Null)
    }

    #[test]
    fn renders_senses_examples_and_attribution() {
        let entries = render(&fixture());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].headword, "colour");
        let EntryBody::Html(html) = &entries[0].body else { panic!("应为 HTML") };
        assert!(html.contains(r#"<span class="phon">/ˈkʌl.ə/</span>"#));
        assert!(html.contains(r#"href="sound://remote/api.dictionaryapi.dev/media/pronunciations/en/colour-uk.mp3""#));
        assert!(html.contains(r#"<div class="pos">noun</div>"#));
        assert!(html.contains(r#"<li class="sense">The spectral composition of visible light."#));
        assert!(html.contains(r#"<span class="x">The leaves change colour in autumn.</span>"#));
        assert!(html.contains("CC BY-SA 3.0"));
        // 两个词源合并成一条
        assert_eq!(html.matches(r#"<span class="hw">"#).count(), 2);
    }

    #[test]
    fn escapes_markup_and_handles_empty() {
        let v: Value = serde_json::json!([{ "word": "<b>", "meanings": [{ "partOfSpeech": "noun", "definitions": [{ "definition": "a & b" }] }] }]);
        let EntryBody::Html(html) = &render(&v)[0].body else { panic!() };
        assert!(html.contains("&lt;b&gt;") && html.contains("a &amp; b"));
        assert!(render(&serde_json::json!([])).is_empty());
        assert!(render(&serde_json::json!({"title": "No Definitions Found"})).is_empty());
    }

    #[test]
    fn page_url_encodes_word() {
        let u = FreeDictionary.page_url("ice cream").map(|u| u.to_string());
        assert_eq!(u.as_deref(), Some("https://api.dictionaryapi.dev/api/v2/entries/en/ice%20cream"));
    }
}
