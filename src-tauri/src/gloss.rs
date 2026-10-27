//! 词汇网络表格视图里每个词的「词性 + 中文释义」：词性来自内置 WordNet，
//! 中文释义从本地双解词典的词条里取第一段中文（跳过例句译文）。

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WordGloss {
    pub word: String,
    /// n. / v. / adj. / adv.，多个用 / 连起来；WordNet 里没有时为 None
    pub pos: Option<String>,
    /// 第一段中文释义；本地词典里没有中文时为 None
    pub gloss: Option<String>,
}

/// 一段中文最长这么多字：再长多半是例句译文
const MAX_RUN: usize = 14;
/// 拼起来的释义最长这么多字
const MAX_GLOSS: usize = 18;

/// 词条 HTML → 纯文本（去掉 <style>、<script> 的内容和所有标签）
pub(crate) fn plain_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 2);
    let mut rest = html;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let lower: String = tail.chars().take(8).collect::<String>().to_lowercase();
        // <style>…</style>、<script>…</script> 整段跳过
        let skip_to = ["style", "script"].iter().find(|t| lower.starts_with(&format!("<{t}"))).map(|t| format!("</{t}"));
        let end = match skip_to {
            Some(close) => tail.to_lowercase().find(&close).and_then(|p| tail[p..].find('>').map(|q| p + q + 1)),
            None => tail.find('>').map(|q| q + 1),
        };
        match end {
            Some(e) => {
                out.push(' ');
                rest = &tail[e..];
            }
            None => {
                rest = "";
            }
        }
    }
    out.push_str(rest);
    crate::senses::decode_entities(&out)
}

pub(crate) fn is_cjk(c: char) -> bool {
    matches!(c, '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}' | '\u{F900}'..='\u{FAFF}')
}

/// 中文释义里会出现的标点（释义之间的分隔、括注）
fn is_gloss_punct(c: char) -> bool {
    matches!(c, '；' | '，' | '、' | '（' | '）' | '…' | '·')
}

/// 收下一段中文：至少两个汉字、不太长、不是以句号结尾的句子、不是前面某段的扩写
fn push_run(runs: &mut Vec<String>, cur: &str, sentence_end: bool) {
    // 两头去掉分隔符；括号要成对留着：（使）氧化
    let sep = |c: char| matches!(c, '；' | '，' | '、' | '…' | '·') || c.is_whitespace();
    let run = cur.trim_start_matches(sep).trim_end_matches(|c: char| sep(c) || c == '（').to_string();
    let n = run.chars().filter(|c| is_cjk(*c)).count();
    let echoes = runs.iter().any(|r| run.contains(r.as_str()));
    if n >= 2 && run.chars().count() <= MAX_RUN && !sentence_end && !echoes {
        runs.push(run);
    }
}

/// 第一段像释义的中文：连续的汉字（可夹释义标点）。以句号结尾、太长的当例句译文跳过；
/// 包含了前面某段释义的也是例句译文（「生锈的」之后的「生锈的金属」）
pub fn chinese_gloss(html: &str) -> Option<String> {
    let text = plain_text(html);
    let mut runs: Vec<String> = Vec::new();
    let mut cur = String::new();
    for c in text.chars() {
        if is_cjk(c) || c == '（' || (is_gloss_punct(c) && !cur.is_empty()) {
            cur.push(c);
        } else if !cur.is_empty() {
            push_run(&mut runs, &cur, matches!(c, '。' | '！' | '？'));
            cur.clear();
        }
        if runs.len() >= 4 {
            break;
        }
    }
    if !cur.is_empty() {
        push_run(&mut runs, &cur, false);
    }
    let first = runs.first()?.clone();
    // 第一段只有一个意思时，再带上后面第一段「释义列表」（生锈的；荒疏；荒废；退步）。
    // 只认带「；」的：例句译文（生了锈的旧汽车）几乎不会有分号
    if first.contains('；') {
        return Some(first);
    }
    match runs.iter().skip(1).find(|r| r.contains('；')) {
        Some(second) if first.chars().count() + second.chars().count() < MAX_GLOSS => Some(format!("{first}；{second}")),
        _ => Some(first),
    }
}

/// 反查索引里每段中文最多这么多个汉字
const MAX_SEGMENT: usize = 12;
/// 每个词条最多收这么多段
const MAX_SEGMENTS: usize = 60;
/// 词典里的语体、词性标签：不当释义收（否则反查「正式」会列出所有标了「正式」的词）
const LABELS: &[&str] = &[
    "名词", "动词", "形容词", "副词", "介词", "连词", "代词", "感叹词", "数词", "冠词", "及物动词", "不及物动词",
    "复数", "单数", "过去式", "过去分词", "现在分词", "比较级", "最高级", "缩写", "简称", "同义词", "反义词",
    "正式", "非正式", "口语", "书面语", "俚语", "贬义", "褒义", "比喻", "幽默", "旧式", "过时", "文学", "术语",
    "英式", "美式", "英国英语", "美国英语", "尤用于", "常用于", "用于", "亦作", "又作", "参见", "见", "例句",
];

/// 按「；，、」切开，括号里的不切：「放弃（权利、要求）」是一段
fn split_top(s: &str) -> Vec<&str> {
    let (mut out, mut depth, mut start) = (Vec::new(), 0i32, 0);
    for (i, c) in s.char_indices() {
        match c {
            '（' => depth += 1,
            '）' => depth = (depth - 1).max(0),
            '；' | '，' | '、' if depth == 0 => {
                out.push(&s[start..i]);
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    out.push(&s[start..]);
    out
}

/// 词条纯文本里所有像释义的中文短语（反查索引用）：
/// 连续的汉字（可夹释义标点）按「；，、」切成段，每段 2–12 个汉字；
/// 以句号、问号、叹号结尾的是例句译文，整段跳过；语体、词性标签不收；同一段只收一次。
pub fn chinese_segments(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let flush = |cur: &str, out: &mut Vec<String>| {
        for seg in split_top(cur) {
            let seg = seg.trim_matches(|c: char| matches!(c, '…' | '·') || c.is_whitespace());
            // 只剩半边括号时去掉：「（使）生锈」保留，「生锈）」→「生锈」
            let seg = if seg.contains('（') == seg.contains('）') { seg } else { seg.trim_matches(['（', '）']) };
            let n = seg.chars().filter(|c| is_cjk(*c)).count();
            if (2..=MAX_SEGMENT).contains(&n) && !LABELS.contains(&seg) && !out.iter().any(|s| s == seg) {
                out.push(seg.to_string());
            }
        }
    };
    for c in text.chars() {
        if is_cjk(c) || c == '（' || (is_gloss_punct(c) && !cur.is_empty()) {
            cur.push(c);
        } else if !cur.is_empty() {
            if !matches!(c, '。' | '！' | '？') {
                flush(&cur, &mut out);
            }
            cur.clear();
            if out.len() >= MAX_SEGMENTS {
                return out;
            }
        }
    }
    if !cur.is_empty() {
        flush(&cur, &mut out);
    }
    out.truncate(MAX_SEGMENTS);
    out
}

/// WordNet 里没有的词，按词尾猜词性（rustily → adv.、rustable → adj.）
pub fn pos_by_suffix(word: &str) -> Option<&'static str> {
    const RULES: &[(&str, &str)] = &[
        ("ly", "adv."),
        ("ness", "n."),
        ("ity", "n."),
        ("ment", "n."),
        ("tion", "n."),
        ("sion", "n."),
        ("ance", "n."),
        ("ence", "n."),
        ("ism", "n."),
        ("ist", "n."),
        ("ship", "n."),
        ("able", "adj."),
        ("ible", "adj."),
        ("less", "adj."),
        ("ful", "adj."),
        ("ous", "adj."),
        ("ive", "adj."),
        ("proof", "adj."),
        ("free", "adj."),
        ("ize", "v."),
        ("ise", "v."),
        ("ify", "v."),
    ];
    let w = word.to_lowercase();
    RULES.iter().find(|(suf, _)| w.len() > suf.len() + 2 && w.ends_with(suf)).map(|(_, p)| *p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_definitions_not_example_translations() {
        let html = r#"<style>.x{color:red}</style><b>rusty</b> <span class="def">covered with rust <chn>生锈的</chn></span>
            <x>rusty metal</x><chn>生锈的金属</chn>
            <span class="def">not as good as it used to be <chn>荒疏；荒废；退步</chn></span>
            <x>My tennis is very rusty these days.</x><chn>最近我的网球荒疏了。</chn>"#;
        assert_eq!(chinese_gloss(html).as_deref(), Some("生锈的；荒疏；荒废；退步"));
        // 例句译文不带分号：不拼上去
        let rust = "<b>rust</b> a reddish-brown substance <chn>锈；铁锈</chn> <x>rusty pipes</x> <chn>生了锈的管子</chn>";
        assert_eq!(chinese_gloss(rust).as_deref(), Some("锈；铁锈"));
        let car = "covered with rust <chn>生锈的</chn> <x>a rusty old car</x> <chn>生了锈的旧汽车</chn>";
        assert_eq!(chinese_gloss(car).as_deref(), Some("生锈的"));
    }

    #[test]
    fn keeps_brackets_paired() {
        assert_eq!(chinese_gloss("oxidize <chn>（使）氧化；（尤指使）生锈</chn>").as_deref(), Some("（使）氧化；（尤指使）生锈"));
    }

    #[test]
    fn guesses_pos_from_suffix() {
        assert_eq!(pos_by_suffix("rustily"), Some("adv."));
        assert_eq!(pos_by_suffix("rustable"), Some("adj."));
        assert_eq!(pos_by_suffix("fly"), None);
    }

    #[test]
    fn skips_sentences_and_long_runs() {
        assert_eq!(chinese_gloss("<p>我很久没有弹钢琴了。</p><p>荒疏</p>").as_deref(), Some("荒疏"));
        assert_eq!(chinese_gloss("<p>English only</p>"), None);
        assert_eq!(chinese_gloss("<p>这是一段非常非常非常非常长的中文例句译文没有句号</p>"), None);
    }

    #[test]
    fn strips_tags_and_entities() {
        assert_eq!(plain_text("<b>a</b>&amp;<script>x()</script>b").split_whitespace().collect::<Vec<_>>(), vec!["a", "&", "b"]);
    }

    #[test]
    fn segments_skip_sentences_and_labels() {
        let text = "ubiquitous adj. 正式 无所不在的；普遍存在的，随处可见的 Mobile phones are ubiquitous. 手机随处可见。 （使）生锈、腐蚀 放弃（权利、要求）";
        assert_eq!(chinese_segments(text), vec!["无所不在的", "普遍存在的", "随处可见的", "（使）生锈", "腐蚀", "放弃（权利、要求）"]);
    }
}
