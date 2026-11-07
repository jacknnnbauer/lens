//! 全书预习的读前导读：从维基百科取书的条目（简介、背景、情节、主题、评价）和作者条目的开头。
//!
//! - 先在英文维基找到这本书：用书名直接取，再用「书名 作者」搜索；条目开头要提到作者的姓才算（搜 Moby-Dick 第一条是那头鲸）。
//! - 想要中文、且有中文条目时取中文条目（转成简体），否则用英文。
//! - 只联网发书名和作者，不发书的内容；结果按书名缓存，Lens 退出前不重复请求。
//! - 维基百科的内容按 CC BY-SA 4.0 许可，导出时注明出处和许可。

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

use crate::error::{Error, Result};

const TIMEOUT: Duration = Duration::from_secs(15);
/// 维基百科要求带上能联系到的 User-Agent：用项目主页，不带用户的任何信息
const USER_AGENT: &str = concat!("Lens/", env!("CARGO_PKG_VERSION"), " (https://github.com/jacknnnbauer/lens)");
/// 每一节最多这么多字（英文条目动辄几万字）；中文按字数少一些
const MAX_SECTION_EN: usize = 2600;
const MAX_SECTION_ZH: usize = 1400;
const MAX_AUTHOR: usize = 1200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// 条目开头的概述
    Lead,
    Background,
    /// 主要人物：常带结局，算剧透
    Characters,
    /// 情节梗概：剧透
    Plot,
    Themes,
    Reception,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    pub kind: Kind,
    /// 条目里的原标题（简介为空）
    pub heading: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Article {
    /// zh | en
    pub lang: String,
    pub title: String,
    pub url: String,
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BookGuide {
    pub book: Article,
    pub author: Option<Article>,
    /// 同一本书在 Grokipedia 上的地址（只给链接：它的使用条款不允许用程序抓取内容，由用户自己打开看）
    pub grokipedia: String,
    /// 情节梗概因为不剧透被略去了
    pub plot_hidden: bool,
    /// AI 写的导读：用的模型、模型说它知不知道这本书；维基百科的为 None
    pub ai: Option<AiNote>,
}

#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiNote {
    pub model: String,
    pub known: bool,
}

/// 主书名：去掉副标题（「Moby-Dick; or, The Whale」「Moby-Dick or, The whale」→ Moby-Dick）和括注
pub fn main_title(title: &str) -> String {
    let mut t = title.trim();
    for sep in [";", ":", " or,", " Or,", " - ", " — ", "(", "（"] {
        if let Some(i) = t.find(sep) {
            if i > 0 {
                t = &t[..i];
            }
        }
    }
    t.trim().trim_end_matches([',', '.']).trim().to_string()
}

/// 作者的姓（Herman Melville → melville），用来确认找到的是这本书的条目
fn surname(author: &str) -> Option<String> {
    let first = author.split([',', ';', '&']).next()?.trim();
    // 「Melville, Herman」这种姓在前的写法
    if author.contains(',') && !author.contains(" and ") {
        return Some(first.to_lowercase()).filter(|s| s.len() >= 2);
    }
    first.split_whitespace().last().map(str::to_lowercase).filter(|s| s.len() >= 2)
}

/// 维基百科纯文本（`exsectionformat=wiki`）切成节：第二级标题算一节，下面的小节并进去
pub fn split_sections(extract: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = vec![(String::new(), String::new())];
    for line in extract.lines() {
        let t = line.trim();
        let level = t.chars().take_while(|&c| c == '=').count();
        let is_heading = level >= 2 && t.len() > level * 2 && t.ends_with(&"=".repeat(level));
        if is_heading {
            let heading = t[level..t.len() - level].trim().to_string();
            if level == 2 {
                out.push((heading, String::new()));
            } else if let Some(last) = out.last_mut() {
                // 小节标题当作一段
                last.1.push_str(&format!("\n{heading}\n"));
            }
            continue;
        }
        if let Some(last) = out.last_mut() {
            last.1.push_str(line);
            last.1.push('\n');
        }
    }
    out.into_iter().map(|(h, t)| (h, tidy(&t))).filter(|(_, t)| !t.is_empty()).collect()
}

/// 去掉多余空行
fn tidy(text: &str) -> String {
    text.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n")
}

/// 按标题认这一节讲什么；参考文献、外部链接、版本、改编这些返回 None（不要）
pub fn classify(heading: &str) -> Option<Kind> {
    let h = heading.to_lowercase();
    let any = |words: &[&str]| words.iter().any(|w| h.contains(w));
    if h.is_empty() {
        return Some(Kind::Lead);
    }
    if any(&[
        "reference", "note", "citation", "external link", "see also", "further reading", "bibliography", "edition", "adaptation", "source", "参考", "注释", "注解",
        "外部链接", "参见", "延伸阅读", "版本", "改编", "资料来源", "脚注", "相关条目", "trivia", "popular culture", "译本", "翻译", "轶闻", "趣闻", "恶搞", "大众文化",
        "名句", "引文",
    ]) {
        return None;
    }
    // 不能只看包含 story：history 里也有
    if any(&["plot", "synopsis", "summary", "情节", "剧情", "故事", "梗概", "内容", "简述", "概要"]) || h == "story" || h.starts_with("story ") {
        return Some(Kind::Plot);
    }
    if any(&["character", "人物", "角色"]) {
        return Some(Kind::Characters);
    }
    if any(&["background", "composition", "writing", "conception", "context", "history", "development", "背景", "创作", "写作", "历史", "成书"]) {
        return Some(Kind::Background);
    }
    if any(&["theme", "style", "analysis", "symbol", "interpretation", "structure", "motif", "主题", "主旨", "风格", "评述", "分析", "象征", "结构", "思想", "艺术"]) {
        return Some(Kind::Themes);
    }
    if any(&["reception", "legacy", "critical", "influence", "impact", "response", "评价", "影响", "反响", "地位", "评论"]) {
        return Some(Kind::Reception);
    }
    Some(Kind::Other)
}

/// 截到 `max` 个字符以内，在段落或句子处断开
fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut: String = text.chars().take(max).collect();
    let at = cut.rfind('\n').filter(|&i| i > max / 2).or_else(|| cut.rfind(['.', '。']).map(|i| i + cut[i..].chars().next().map_or(1, char::len_utf8)));
    format!("{}…", cut[..at.unwrap_or(cut.len())].trim_end())
}

/// 把条目整理成导读的几节：按讲什么排好、截短；不剧透时去掉情节梗概；条目短时（中文条目）没归类的节也留着
pub fn build_sections(extract: &str, lang: &str, spoilers: bool) -> (Vec<Section>, bool) {
    let max = if lang == "zh" { MAX_SECTION_ZH } else { MAX_SECTION_EN };
    let short = extract.chars().count() < 15_000;
    let mut hidden = false;
    let mut out: Vec<Section> = Vec::new();
    for (heading, text) in split_sections(extract) {
        let Some(kind) = classify(&heading) else { continue };
        if matches!(kind, Kind::Plot | Kind::Characters) && !spoilers {
            hidden = true;
            continue;
        }
        // 只剩几个字的节（「第一章 / 第六章」）没用
        if kind != Kind::Lead && text.chars().count() < 40 {
            continue;
        }
        if kind == Kind::Other && !short {
            continue;
        }
        out.push(Section { kind, heading, text: clip(&text, max) });
    }
    let rank = |k: Kind| match k {
        Kind::Lead => 0,
        Kind::Background => 1,
        Kind::Characters => 2,
        Kind::Plot => 3,
        Kind::Themes => 4,
        Kind::Reception => 5,
        Kind::Other => 6,
    };
    out.sort_by_key(|s| rank(s.kind));
    (out, hidden)
}

/// 中文条目的标题常是繁体（白鯨記）：取简体正文开头书名号里的名字（《白鲸》）
fn display_title(lang: &str, title: &str, text: &str) -> String {
    if lang != "zh" {
        return title.to_string();
    }
    let head: String = text.chars().take(80).collect();
    match (head.find('《'), head.find('》')) {
        (Some(a), Some(b)) if b > a + 3 => head[a + 3..b].to_string(),
        _ => title.to_string(),
    }
}

/// Grokipedia 的条目地址和英文维基的标题一样（空格换成下划线）
pub fn grokipedia_url(en_title: &str) -> String {
    let t = en_title.trim().replace(' ', "_");
    let mut u = url::Url::parse("https://grokipedia.com/page/").expect("固定的地址");
    u.path_segments_mut().map(|mut p| {
        p.pop_if_empty().push(&t);
    }).ok();
    u.to_string()
}

/// 同一本书（书名、作者、语言、剧透）只取一次
type Key = (String, String, String, bool);
static CACHE: Mutex<Option<HashMap<Key, Option<BookGuide>>>> = Mutex::new(None);

pub fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder().user_agent(USER_AGENT).timeout(TIMEOUT).build().map_err(|e| Error::Online(e.to_string()))
}

async fn api(client: &reqwest::Client, lang: &str, params: &[(&str, &str)]) -> Result<Value> {
    let mut q: Vec<(&str, &str)> = vec![("format", "json"), ("formatversion", "2"), ("redirects", "1")];
    q.extend_from_slice(params);
    let url = url::Url::parse_with_params(&format!("https://{lang}.wikipedia.org/w/api.php"), &q).map_err(|e| Error::Online(e.to_string()))?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| Error::Online(crate::tf!("连不上维基百科：{}", "Could not reach Wikipedia: {}", e)))?;
    if !resp.status().is_success() {
        return Err(Error::Online(crate::tf!("维基百科返回 {}", "Wikipedia returned {}", resp.status().as_u16())));
    }
    resp.json().await.map_err(|e| Error::Online(e.to_string()))
}

fn first_page(v: &Value) -> Option<&Value> {
    v.pointer("/query/pages/0").filter(|p| p.get("missing").is_none() && p.get("invalid").is_none())
}

/// 条目开头一段（判断是不是这本书）和是否消歧义页
async fn lead_of(client: &reqwest::Client, title: &str) -> Result<Option<(String, String)>> {
    let v = api(client, "en", &[("action", "query"), ("prop", "extracts|pageprops"), ("exintro", "1"), ("explaintext", "1"), ("titles", title)]).await?;
    let Some(p) = first_page(&v) else { return Ok(None) };
    if p.pointer("/pageprops/disambiguation").is_some() {
        return Ok(None);
    }
    let real = p.get("title").and_then(Value::as_str).unwrap_or(title).to_string();
    Ok(Some((real, p.get("extract").and_then(Value::as_str).unwrap_or_default().to_string())))
}

/// 在英文维基里找到这本书的条目标题
async fn find_book(client: &reqwest::Client, title: &str, author: Option<&str>) -> Result<Option<String>> {
    let main = main_title(title);
    if main.is_empty() {
        return Ok(None);
    }
    let sur = author.and_then(surname);
    let fits = |lead: &str| match &sur {
        Some(s) => lead.to_lowercase().contains(s.as_str()),
        // 不知道作者：开头像是在讲一本书
        None => {
            let l = lead.to_lowercase();
            ["novel", "book", "novella", "memoir", "play", "poem", "collection"].iter().any(|w| l.contains(w))
        }
    };
    let mut candidates = vec![main.clone()];
    let query = format!("{main} {}", author.unwrap_or("novel"));
    let v = api(client, "en", &[("action", "query"), ("list", "search"), ("srsearch", &query), ("srlimit", "6"), ("srprop", "")]).await?;
    if let Some(hits) = v.pointer("/query/search").and_then(Value::as_array) {
        for h in hits {
            if let Some(t) = h.get("title").and_then(Value::as_str) {
                if !candidates.iter().any(|c| c == t) {
                    candidates.push(t.to_string());
                }
            }
        }
    }
    for c in candidates.iter().take(5) {
        if let Some((real, lead)) = lead_of(client, c).await? {
            if fits(&lead) {
                return Ok(Some(real));
            }
        }
    }
    Ok(None)
}

/// 取整篇条目（纯文本、带节标题）、地址和对应的中文条目标题
async fn fetch_article(client: &reqwest::Client, lang: &str, title: &str, intro_only: bool) -> Result<Option<(String, String, String, Option<String>)>> {
    let mut params = vec![("action", "query"), ("prop", "extracts|info|langlinks"), ("inprop", "url"), ("explaintext", "1"), ("exsectionformat", "wiki"), ("lllang", "zh"), ("titles", title)];
    if intro_only {
        params.push(("exintro", "1"));
    }
    if lang == "zh" {
        // 繁简转换：取简体
        params.push(("variant", "zh-cn"));
    }
    let v = api(client, lang, &params).await?;
    let Some(p) = first_page(&v) else { return Ok(None) };
    let get = |k: &str| p.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    let zh = p.pointer("/langlinks/0/title").and_then(Value::as_str).map(str::to_string);
    let extract = get("extract");
    if extract.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some((get("title"), get("fullurl"), extract, zh)))
}

/// 书和作者的读前导读；维基百科上找不到这本书时返回 None
pub async fn book_guide(title: &str, author: Option<&str>, want_zh: bool, spoilers: bool) -> Result<Option<BookGuide>> {
    let key: Key = (title.to_string(), author.unwrap_or_default().to_string(), if want_zh { "zh" } else { "en" }.to_string(), spoilers);
    if let Ok(cache) = CACHE.lock() {
        if let Some(hit) = cache.as_ref().and_then(|m| m.get(&key)) {
            return Ok(hit.clone());
        }
    }
    let client = client()?;
    let guide = match find_book(&client, title, author).await? {
        None => None,
        Some(en_title) => {
            let Some((en_real, en_url, en_text, zh_title)) = fetch_article(&client, "en", &en_title, false).await? else { return Ok(None) };
            let grokipedia = grokipedia_url(&en_real);
            let mut book = None;
            if want_zh {
                if let Some(zt) = zh_title.as_deref() {
                    if let Some((t, url, text, _)) = fetch_article(&client, "zh", zt, false).await? {
                        let (sections, hidden) = build_sections(&text, "zh", spoilers);
                        book = Some((Article { lang: "zh".into(), title: display_title("zh", &t, &text), url, sections }, hidden));
                    }
                }
            }
            let (book, plot_hidden) = match book {
                Some(b) => b,
                None => {
                    let (sections, hidden) = build_sections(&en_text, "en", spoilers);
                    (Article { lang: "en".into(), title: en_real, url: en_url, sections }, hidden)
                }
            };
            let author = match author.filter(|a| !a.trim().is_empty()) {
                Some(a) => author_article(&client, a, want_zh).await.unwrap_or(None),
                None => None,
            };
            Some(BookGuide { book, author, plot_hidden, grokipedia, ai: None })
        }
    };
    if let Ok(mut cache) = CACHE.lock() {
        cache.get_or_insert_with(HashMap::new).insert(key, guide.clone());
    }
    Ok(guide)
}

/// 作者条目的开头一段：先确认英文条目是个作家，再按需要换成中文
async fn author_article(client: &reqwest::Client, author: &str, want_zh: bool) -> Result<Option<Article>> {
    let name = author.split([';', '&']).next().unwrap_or(author).trim();
    let Some((en_title, en_url, en_text, zh_title)) = fetch_article(client, "en", name, true).await? else { return Ok(None) };
    let l = en_text.to_lowercase();
    if !["writer", "novelist", "author", "poet", "playwright", "essayist", "journalist"].iter().any(|w| l.contains(w)) {
        return Ok(None);
    }
    if want_zh {
        if let Some(zt) = zh_title {
            if let Some((t, url, text, _)) = fetch_article(client, "zh", &zt, true).await? {
                return Ok(Some(Article { lang: "zh".into(), title: t, url, sections: vec![Section { kind: Kind::Lead, heading: String::new(), text: clip(&tidy(&text), MAX_AUTHOR) }] }));
            }
        }
    }
    Ok(Some(Article { lang: "en".into(), title: en_title, url: en_url, sections: vec![Section { kind: Kind::Lead, heading: String::new(), text: clip(&tidy(&en_text), MAX_AUTHOR) }] }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 联网实测：`cargo test --lib wiki::tests::live -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live() -> Result<()> {
        for (title, author, zh, spoilers) in [
            ("Moby-Dick or, The whale", Some("Herman Melville"), true, false),
            ("Moby-Dick or, The whale", Some("Herman Melville"), false, true),
            ("Pride and Prejudice", Some("Jane Austen"), true, true),
            ("No Such Book Zxqv", Some("Nobody Qq"), true, false),
        ] {
            let g = tauri::async_runtime::block_on(book_guide(title, author, zh, spoilers))?;
            match g {
                None => println!("== {title}: not found"),
                Some(g) => {
                    println!("== {title} [{}] {} {} plot_hidden={}", g.book.lang, g.book.title, g.book.url, g.plot_hidden);
                    for s in &g.book.sections {
                        println!("  {:?} «{}» {} chars: {}", s.kind, s.heading, s.text.chars().count(), s.text.chars().take(60).collect::<String>().replace('\n', " / "));
                    }
                    if let Some(a) = &g.author {
                        println!("  author [{}] {}: {}", a.lang, a.title, a.sections[0].text.chars().take(80).collect::<String>());
                    }
                }
            }
        }
        Ok(())
    }

    #[test]
    fn main_title_drops_subtitles() {
        assert_eq!(main_title("Moby-Dick or, The whale"), "Moby-Dick");
        assert_eq!(main_title("Moby-Dick; or, The Whale"), "Moby-Dick");
        assert_eq!(main_title("Frankenstein: or, The Modern Prometheus"), "Frankenstein");
        assert_eq!(main_title("The Great Gatsby (Penguin Classics)"), "The Great Gatsby");
        assert_eq!(surname("Herman Melville").as_deref(), Some("melville"));
        assert_eq!(surname("Melville, Herman").as_deref(), Some("melville"));
    }

    const EXTRACT: &str = "Moby-Dick is an 1851 novel by Herman Melville.\n\n\n== Plot ==\nIshmael goes to sea on the whaler Pequod.\nThe ship sinks at the end.\n\n== Themes ==\nObsession and revenge drive the captain.\n\n=== Style ===\nRich prose.\n\n== Background ==\nMelville sailed on whalers in the early 1840s.\n\n== Reception ==\nMixed reviews at first, a classic later on.\n\n== Adaptations ==\nFilms.\n\n== Legacy ==\nShort.\n\n== References ==\nX\n";

    #[test]
    fn sections_are_grouped_classified_and_ordered() {
        let (s, hidden) = build_sections(EXTRACT, "en", true);
        let kinds: Vec<Kind> = s.iter().map(|x| x.kind).collect();
        assert_eq!(kinds, [Kind::Lead, Kind::Background, Kind::Plot, Kind::Themes, Kind::Reception]);
        assert!(!hidden);
        assert_eq!(s[3].text, "Obsession and revenge drive the captain.\nStyle\nRich prose.", "小节并进上一节");
        assert!(s.iter().all(|x| x.heading != "Legacy"), "只剩几个字的节不要");
        // 不剧透：去掉情节
        let (s, hidden) = build_sections(EXTRACT, "en", false);
        assert!(hidden && s.iter().all(|x| x.kind != Kind::Plot));
    }

    #[test]
    fn chinese_headings_and_clipping() {
        assert_eq!(classify("故事简述"), Some(Kind::Plot));
        assert_eq!(classify("评述"), Some(Kind::Themes));
        assert_eq!(classify("影响"), Some(Kind::Reception));
        assert_eq!(classify("参考文献"), None);
        assert_eq!(classify("莫比·迪克名字的来源"), Some(Kind::Other));
        assert_eq!(classify("Publication history"), Some(Kind::Background), "history 不是 story");
        assert_eq!(classify("主要角色"), Some(Kind::Characters));
        assert_eq!(classify("中文译本"), None);
        assert_eq!(display_title("zh", "白鯨記", "《白鲸》（英语：Moby-Dick）是"), "白鲸");
        assert_eq!(display_title("en", "Moby-Dick", "《x》"), "Moby-Dick");
        assert_eq!(grokipedia_url("Moby-Dick"), "https://grokipedia.com/page/Moby-Dick");
        assert_eq!(grokipedia_url("Pride and Prejudice"), "https://grokipedia.com/page/Pride_and_Prejudice");
        let long = "第一句。第二句。".repeat(200);
        let c = clip(&long, 50);
        assert!(c.chars().count() <= 51 && c.ends_with('…'));
        assert!(c.trim_end_matches('…').ends_with('。'));
    }
}
