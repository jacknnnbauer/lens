//! 网页模式：用户填写含 `{word}` 的网址模板，在只读窗口里打开原页面（SPEC 13）。
//! 不抓取、不解析页面内容；结果不参与复制聚合与导出。

use async_trait::async_trait;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use url::Url;

use super::{Ctx, OnlineMode, OnlineProvider};
use crate::dict::RawEntry;
use crate::error::{Error, Result};

pub const ID: &str = "web";
const PLACEHOLDER: &str = "{word}";

pub struct WebDict {
    template: String,
}

impl WebDict {
    /// 模板必须是 http(s) 地址且含 `{word}`。
    pub fn new(template: &str) -> Result<Self> {
        let t = template.trim();
        if !t.contains(PLACEHOLDER) {
            return Err(Error::Online(crate::i18n::t("网址模板里要有 {word}，查词时替换成单词", "The URL template must contain {word}; it is replaced by the looked-up word").into()));
        }
        let probe = Url::parse(&t.replace(PLACEHOLDER, "test")).map_err(|_| Error::Online(crate::i18n::t("网址模板不是有效的网址", "The URL template is not a valid URL").into()))?;
        if !matches!(probe.scheme(), "https" | "http") || probe.host_str().is_none() {
            return Err(Error::Online(crate::i18n::t("网址模板只能是 http:// 或 https:// 开头", "The URL template must start with http:// or https://").into()));
        }
        Ok(Self { template: t.to_string() })
    }
}

#[async_trait]
impl OnlineProvider for WebDict {
    fn id(&self) -> &'static str {
        ID
    }

    fn mode(&self) -> OnlineMode {
        OnlineMode::Web
    }

    async fn lookup(&self, _: &Ctx<'_>, _: &str) -> Result<Vec<RawEntry>> {
        Ok(Vec::new())
    }

    fn page_url(&self, word: &str) -> Option<Url> {
        let encoded = utf8_percent_encode(word.trim(), NON_ALPHANUMERIC).to_string();
        Url::parse(&self.template.replace(PLACEHOLDER, &encoded)).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_template() {
        assert!(WebDict::new("https://dictionary.cambridge.org/dictionary/english/{word}").is_ok());
        assert!(WebDict::new("https://example.com/").is_err());
        assert!(WebDict::new("javascript:alert('{word}')").is_err());
        assert!(WebDict::new("file:///C:/{word}").is_err());
        assert!(WebDict::new("not a url {word}").is_err());
    }

    #[test]
    fn substitutes_encoded_word() -> Result<()> {
        let d = WebDict::new("https://example.com/search?q={word}&lang=en")?;
        assert_eq!(d.page_url(" ice cream ").map(|u| u.to_string()).as_deref(), Some("https://example.com/search?q=ice%20cream&lang=en"));
        let d = WebDict::new("https://example.com/dict/{word}")?;
        assert_eq!(d.page_url("a/b?c").map(|u| u.to_string()).as_deref(), Some("https://example.com/dict/a%2Fb%3Fc"));
        Ok(())
    }
}
