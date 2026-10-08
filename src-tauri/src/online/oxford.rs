//! Oxford Dictionaries API v2（牛津官方，需 App ID + App Key）。
//! GET https://od-api.oxforddictionaries.com/api/v2/entries/en-gb/<word_id>，请求头 app_id / app_key。
//! 屈折形式（running）没有自己的词条：先查 lemmas 端点拿到原形再查。
//! 试用沙盒（od-api-sandbox）共 500 次调用，且只返回 a 开头的词。

use async_trait::async_trait;
use serde_json::Value;
use url::Url;

use super::{esc, get_json, sound_link, Credential, Ctx, KeyKind, OnlineMode, OnlineProvider, STYLE};
use crate::dict::{EntryBody, RawEntry};
use crate::error::{Error, Result};

pub const ID: &str = "oxford";
const PRODUCTION: &str = "https://od-api.oxforddictionaries.com/api/v2/";
const SANDBOX: &str = "https://od-api-sandbox.oxforddictionaries.com/api/v2/";
const LANG: &str = "en-gb";

pub struct Oxford {
    pub sandbox: bool,
}

impl Oxford {
    fn endpoint(&self, kind: &str, word: &str) -> Result<Url> {
        let mut url = Url::parse(if self.sandbox { SANDBOX } else { PRODUCTION }).map_err(|e| Error::Online(e.to_string()))?;
        url.path_segments_mut()
            .map_err(|_| Error::Online(crate::i18n::t("地址无效", "Invalid address").into()))?
            .pop_if_empty()
            .extend([kind, LANG, &word.trim().to_lowercase()]);
        Ok(url)
    }

    async fn get(&self, ctx: &Ctx<'_>, cred: &Credential, url: Url) -> Result<Option<Value>> {
        let id = cred.id.as_deref().unwrap_or_default();
        get_json(ctx.client.get(url).header("app_id", id).header("app_key", &cred.key)).await
    }
}

#[async_trait]
impl OnlineProvider for Oxford {
    fn id(&self) -> &'static str {
        ID
    }

    fn mode(&self) -> OnlineMode {
        OnlineMode::Api
    }

    fn key_kind(&self) -> KeyKind {
        KeyKind::IdAndKey
    }

    async fn lookup(&self, ctx: &Ctx<'_>, word: &str) -> Result<Vec<RawEntry>> {
        let cred = ctx.cred.ok_or_else(|| Error::Online(crate::i18n::t("缺少 App ID / App Key", "Missing App ID / App Key").into()))?;
        let mut entries = self.endpoint("entries", word)?;
        entries.query_pairs_mut().append_pair("strictMatch", "false");
        if let Some(v) = self.get(ctx, cred, entries).await? {
            return Ok(render(&v));
        }
        // 屈折形式 → 原形
        let Some(lemmas) = self.get(ctx, cred, self.endpoint("lemmas", word)?).await? else { return Ok(Vec::new()) };
        let Some(root) = lemma_of(&lemmas).filter(|r| !r.eq_ignore_ascii_case(word.trim())) else { return Ok(Vec::new()) };
        let mut url = self.endpoint("entries", &root)?;
        url.query_pairs_mut().append_pair("strictMatch", "false");
        Ok(self.get(ctx, cred, url).await?.map(|v| render(&v)).unwrap_or_default())
    }

    fn page_url(&self, word: &str) -> Option<Url> {
        let mut url = Url::parse("https://www.oxfordlearnersdictionaries.com/search/english/").ok()?;
        url.query_pairs_mut().append_pair("q", word.trim());
        Some(url)
    }

    fn media_hosts(&self) -> &'static [&'static str] {
        &["audio.oxforddictionaries.com"]
    }
}

fn arr<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key).and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default()
}

fn lemma_of(json: &Value) -> Option<String> {
    let le = arr(json, "results").first().map(|r| arr(r, "lexicalEntries"))?.first()?;
    arr(le, "inflectionOf").first()?.get("id")?.as_str().map(str::to_string)
}

fn first_pronunciation(lists: &[&[Value]]) -> Option<(Option<String>, Option<String>)> {
    lists.iter().flat_map(|l| l.iter()).find_map(|p| {
        let ipa = p.get("phoneticSpelling").and_then(Value::as_str).map(str::to_string);
        let audio = p.get("audioFile").and_then(Value::as_str).map(str::to_string);
        (ipa.is_some() || audio.is_some()).then_some((ipa, audio))
    })
}

fn senses(list: &[Value], html: &mut String) {
    if list.is_empty() {
        return;
    }
    html.push_str("<ol>");
    for s in list {
        let defs: Vec<&str> = arr(s, "definitions").iter().filter_map(Value::as_str).collect();
        let short: Vec<&str> = arr(s, "shortDefinitions").iter().filter_map(Value::as_str).collect();
        let text = if defs.is_empty() { short } else { defs };
        if text.is_empty() && arr(s, "subsenses").is_empty() {
            continue;
        }
        html.push_str(&format!("<li class=\"sense\">{}", esc(&text.join("; "))));
        for ex in arr(s, "examples") {
            if let Some(t) = ex.get("text").and_then(Value::as_str) {
                html.push_str(&format!("<span class=\"x\">{}</span>", esc(t)));
            }
        }
        senses(arr(s, "subsenses"), html);
        html.push_str("</li>");
    }
    html.push_str("</ol>");
}

pub fn render(json: &Value) -> Vec<RawEntry> {
    let mut out = Vec::new();
    for result in arr(json, "results") {
        let word = result.get("word").and_then(Value::as_str).unwrap_or_default();
        let lexical = arr(result, "lexicalEntries");
        if lexical.is_empty() {
            continue;
        }
        let mut html = format!("{STYLE}<div class=\"lens-online\"><div class=\"hwline\"><span class=\"hw\">{}</span>", esc(word));
        let mut lists: Vec<&[Value]> = vec![arr(result, "pronunciations")];
        for le in lexical {
            lists.push(arr(le, "pronunciations"));
            for e in arr(le, "entries") {
                lists.push(arr(e, "pronunciations"));
            }
        }
        if let Some((ipa, audio)) = first_pronunciation(&lists) {
            if let Some(ipa) = ipa {
                html.push_str(&format!("<span class=\"phon\">/{}/</span>", esc(&ipa)));
            }
            if let Some(a) = audio {
                html.push_str(&sound_link(&a));
            }
        }
        html.push_str("</div>");
        for le in lexical {
            if let Some(pos) = le.get("lexicalCategory").and_then(|c| c.get("text")).and_then(Value::as_str) {
                html.push_str(&format!("<div class=\"pos\">{}</div>", esc(pos)));
            }
            for e in arr(le, "entries") {
                senses(arr(e, "senses"), &mut html);
            }
        }
        html.push_str("<p class=\"source\">来源：Oxford Dictionaries API · © Oxford University Press</p></div>");
        out.push(RawEntry { headword: word.to_string(), body: EntryBody::Html(html) });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_results_with_subsenses() {
        let v: Value = serde_json::from_str(include_str!("../../../fixtures/online/oxford-ace.json")).unwrap_or(Value::Null);
        let entries = render(&v);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].headword, "ace");
        let EntryBody::Html(html) = &entries[0].body else { panic!() };
        assert!(html.contains("<span class=\"phon\">/eɪs/</span>"));
        assert!(html.contains("sound://remote/audio.oxforddictionaries.com/en/mp3/ace__gb_1.mp3"));
        assert!(html.contains("<div class=\"pos\">Noun</div>"));
        assert!(html.contains("<span class=\"x\">the ace of diamonds</span>"));
        assert!(html.contains("a person who excels at a particular activity"));
        assert!(html.contains("<div class=\"pos\">Adjective</div>"));
    }

    #[test]
    fn lemma_lookup() {
        let v = serde_json::json!({"results":[{"lexicalEntries":[{"inflectionOf":[{"id":"run","text":"run"}]}]}]});
        assert_eq!(lemma_of(&v).as_deref(), Some("run"));
        assert_eq!(lemma_of(&serde_json::json!({})), None);
    }

    #[test]
    fn endpoints() -> Result<()> {
        assert_eq!(Oxford { sandbox: false }.endpoint("entries", "Ace")?.as_str(), "https://od-api.oxforddictionaries.com/api/v2/entries/en-gb/ace");
        assert!(Oxford { sandbox: true }.endpoint("lemmas", "a b")?.as_str().ends_with("od-api-sandbox.oxforddictionaries.com/api/v2/lemmas/en-gb/a%20b"));
        Ok(())
    }
}
