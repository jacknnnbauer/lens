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
    // 只数汉字：「（贸然）上前搭讪；（唐突地）走近谈话」的括号、分号不算长度
    let n = run.chars().filter(|c| is_cjk(*c)).count();
    let echoes = runs.iter().any(|r| run.contains(r.as_str()));
    if (2..=MAX_RUN).contains(&n) && !sentence_end && !echoes && !LABELS.contains(&run.as_str()) {
        runs.push(run);
    }
}

/// 第一段像释义的中文：连续的汉字（可夹释义标点）。以句号结尾、太长的当例句译文跳过；
/// 包含了前面某段释义的也是例句译文（「生锈的」之后的「生锈的金属」）
pub fn chinese_gloss(html: &str) -> Option<String> {
    // 牛津、朗文这类把每个义项的中文单独标出来的：直接取第一个义项
    if let Some(first) = chinese_senses(html).into_iter().next() {
        return Some(first);
    }
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

/// 一个词最多列这么多个义项
const MAX_SENSES: usize = 4;
/// 一个义项的中文最多这么多个汉字（再长多半标错了）
const MAX_SENSE_CJK: usize = 40;

/// 双解词典里按义项标好的中文释义：牛津高阶、朗文当代都把每个义项的中文放在 `class="chn"` 的元素里，
/// 例句译文另用 tx / xc。按出现顺序每个义项一段，去掉重复和语体标签；词典没这样标时返回空，调用方退回按纯文本找。
///
/// 同样标成 chn 的还有不是释义的中文，按它所在的元素分辨：
/// - 牛津：释义在 `d` / `ud` 里；`dc`（用于人、用于鸟这类使用对象）、`u`（不用于进行时这类语法说明）不要；
/// - 习语、短语动词另成一组（牛津 `sense-g` / `idm-g` / `pv-g`，朗文 `phrgrp`），不是这个词本身的意思，不要
pub fn chinese_senses(html: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut from = 0;
    while let Some(p) = html[from..].find("class=\"chn\"").map(|p| p + from) {
        from = p + 1;
        let Some(lt) = html[..p].rfind('<') else { continue };
        let ancestors = ancestor_classes(html, lt, 3);
        let parent = ancestors.first().map(String::as_str).unwrap_or_default();
        let not_a_sense = matches!(parent, "dc" | "u" | "gu" | "label-g" | "cf" | "x" | "x-g" | "tx" | "xc" | "example")
            || ancestors.iter().any(|a| matches!(a.as_str(), "sense-g" | "idm-g" | "idm" | "pv-g" | "phrgrp" | "x-g" | "example"));
        if not_a_sense {
            continue;
        }
        let name: String = html[lt + 1..].chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
        let Some(gt) = html[p..].find('>').map(|g| p + g + 1) else { break };
        let Some(end) = closing_tag(html, gt, &name) else { break };
        let sense = clean_sense(&plain_text(&html[gt..end]));
        let n = sense.chars().filter(|c| is_cjk(*c)).count();
        // 「* fish 是通常的复数形式…」这类用法注释也标成了 chn
        let note = sense.starts_with(['*', '※', '注']);
        if (1..=MAX_SENSE_CJK).contains(&n) && !note && !LABELS.contains(&sense.as_str()) && !out.contains(&sense) {
            out.push(sense);
            if out.len() >= MAX_SENSES {
                break;
            }
        }
        from = end;
    }
    out
}

/// 位置 `lt` 处的元素往外几层祖先元素的 class（第一个 class 名；没有 class 的记空串）
fn ancestor_classes(html: &str, lt: usize, n: usize) -> Vec<String> {
    const VOID: &[&str] = &["img", "br", "link", "meta", "hr", "input", "source", "wbr", "area", "col"];
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut end = lt;
    while out.len() < n {
        let Some(open) = html[..end].rfind('<') else { break };
        let tag = &html[open..];
        let close = tag.find('>').map_or(tag.len(), |g| g + 1);
        let tag = &tag[..close];
        end = open;
        if tag.starts_with("</") {
            depth += 1;
            continue;
        }
        if tag.starts_with("<!") || tag.ends_with("/>") {
            continue;
        }
        let name: String = tag[1..].chars().take_while(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
        if name.is_empty() || VOID.contains(&name.as_str()) {
            continue;
        }
        if depth > 0 {
            depth -= 1;
            continue;
        }
        let class = tag
            .find("class=\"")
            .map(|i| &tag[i + 7..])
            .and_then(|rest| rest.find('"').map(|j| &rest[..j]))
            .and_then(|c| c.split_whitespace().next())
            .unwrap_or_default();
        out.push(class.to_string());
    }
    out
}

/// 从 `start` 起找和开标签配对的 `</name`（中间可以套同名标签），返回它的位置
fn closing_tag(html: &str, start: usize, name: &str) -> Option<usize> {
    if name.is_empty() {
        return None;
    }
    let (open, close) = (format!("<{name}"), format!("</{name}"));
    let mut depth = 1;
    let mut i = start;
    while let Some(lt) = html[i..].find('<').map(|q| q + i) {
        let rest = &html[lt..];
        if rest.starts_with(&close) {
            depth -= 1;
            if depth == 0 {
                return Some(lt);
            }
        } else if rest.starts_with(&open) && rest[open.len()..].starts_with(|c: char| c == '>' || c.is_whitespace()) {
            depth += 1;
        }
        i = lt + 1;
    }
    None
}

/// 一个义项的中文：去掉朗文那种长的〔使用场合〕说明（短的〔陌生人〕是释义的一部分，留着）、末尾的冒号分号、多余空白
fn clean_sense(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '〔' {
            out.push(c);
            continue;
        }
        let inner: String = chars.by_ref().take_while(|&x| x != '〕').collect();
        if inner.chars().filter(|x| is_cjk(*x)).count() <= 5 {
            out.push('〔');
            out.push_str(&inner);
            out.push('〕');
        }
    }
    // 去标签时补的空格：挨着中文的去掉，英文词之间的留一个
    let words: Vec<&str> = out.split_whitespace().collect();
    let mut out = String::new();
    for (i, w) in words.iter().enumerate() {
        let prev_ascii = i > 0 && words[i - 1].chars().last().is_some_and(|c| c.is_ascii());
        if prev_ascii && w.chars().next().is_some_and(|c| c.is_ascii()) {
            out.push(' ');
        }
        out.push_str(w);
    }
    out.trim_matches(|c: char| matches!(c, '：' | ':' | '；' | ';' | '，' | ',' | '。') || c.is_whitespace()).to_string()
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
    "书面", "口语体", "正式用语", "非正式用语", "古", "旧", "旧用法", "古旧用法", "文", "方言", "谑", "讳", "粗", "忌", "儿语", "诗", "术",
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

/// 派生词可能的原词：justly → just、hastily → hasty、swiftness → swift、happiness → happy、seamen → seaman、
/// legibly → legible。按可能性排列，不检查是否存在
pub fn derivation_bases(word: &str) -> Vec<String> {
    let w = word.to_lowercase();
    let mut out = Vec::new();
    let mut add = |stem: Option<&str>, tail: &str| {
        if let Some(s) = stem.filter(|s| s.chars().count() >= 3) {
            let b = format!("{s}{tail}");
            if b != w && !out.contains(&b) {
                out.push(b);
            }
        }
    };
    add(w.strip_suffix("ibly"), "ible");
    add(w.strip_suffix("ably"), "able");
    add(w.strip_suffix("ily"), "y");
    add(w.strip_suffix("ly"), "");
    add(w.strip_suffix("iness"), "y");
    add(w.strip_suffix("ness"), "");
    add(w.strip_suffix("men"), "man");
    add(w.strip_suffix("ishly"), "");
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

    /// 牛津高阶（abundantly）：两个义项，例句译文用 tx
    const OALD: &str = r#"<span class="n-g"><span class="z_n">1</span> <span class="cc">~ clear</span> <span class="def-g"><span class="d">very clear <span localeuidoupc="201" class="chn">十分清楚；非常明白</span> </span></span>
        <span class="x-g"><span class="x">She made her wishes abundantly clear.</span> <span class="tx">她充分表明了她的意愿。</span></span></span>
        <span class="n-g"><span class="z_n">2</span><span class="def-g"><span class="d">in large quantities <span class="chn">大量地；丰盛地；充裕地</span> </span></span></span>"#;
    /// 朗文当代（accost）：语体标签【书面】在 gc 里，长的〔使用场合〕说明在中文前面
    const LDOCE: &str = r#"<span class="gx">written </span gx><div class="define"><div class="nataccep">to go towards someone <span class="gc">【书面】</span gc><span class="chn">〔以不怀好意的态度〕上前和〔陌生人〕说话：</span chn></div nataccep>
        <span class="example"><span class="xe">He was accosted by four youths.</span xe><span class="xc">四个年轻人上前跟他搭讪。 </span xc></span example></div define>"#;

    #[test]
    fn reads_each_sense_of_bilingual_dictionaries() {
        assert_eq!(chinese_senses(OALD), ["十分清楚；非常明白", "大量地；丰盛地；充裕地"]);
        assert_eq!(chinese_senses(LDOCE), ["上前和〔陌生人〕说话"]);
        // 简释取第一个义项，不再取到【书面】
        assert_eq!(chinese_gloss(LDOCE).as_deref(), Some("上前和〔陌生人〕说话"));
        // 牛津的长释义带括号和分号，汉字不多就不算太长
        let accost = r#"<span class="d">to go up to sb <span class="chn">（贸然）上前搭讪；（唐突地）走近谈话</span></span>"#;
        assert_eq!(chinese_gloss(accost).as_deref(), Some("（贸然）上前搭讪；（唐突地）走近谈话"));
        assert_eq!(chinese_gloss("<p>【书面】</p><p>（贸然）上前搭讪；（唐突地）走近谈话</p>").as_deref(), Some("（贸然）上前搭讪；（唐突地）走近谈话"));
        // 同名标签套在里面也能找到配对的结束标签
        assert_eq!(chinese_senses(r#"<span class="d"><span class="chn">打滚<span>，</span>嬉戏</span></span><span class="d"><span class="chn">打滚，嬉戏</span></span>"#), ["打滚，嬉戏"]);
        assert!(chinese_senses("<b>no marks</b> 中文").is_empty());
    }

    #[test]
    fn skips_chinese_that_is_not_a_sense() {
        // 牛津 wallow：「(of large animals or people)」的中文在 dc 里；习语在 sense-g 里；abominate 的「不用于进行时」在 u 里
        let wallow = r#"<span class="n-g"><span class="def-g"><span class="dc">(of large animals or people <span class="chn">大动物或人</span>)</span>
            <span class="d">to lie and roll about in water <span class="chn">（为保持凉爽在烂泥、水里）打滚，翻滚</span></span></span></span>
            <span class="n-g"><span class="def-g"><span class="d">to enjoy something <span class="chn">沉湎；放纵</span></span></span></span>
            <span class="sense-g"><span class="def-g"><span class="d">a saying <span class="chn">本性难改</span></span></span></span>
            <span class="label-g">(<span class="u">not used in the progressive <span class="chn">不用于进行时</span></span>)</span>"#;
        assert_eq!(chinese_senses(wallow), ["（为保持凉爽在烂泥、水里）打滚，翻滚", "沉湎；放纵"]);
        // 朗文：短语动词在 phrgrp 里
        let peck = r#"<div class="define"><div class="nataccep">to bite <span class="chn">〔鸟〕啄，啄食：</span chn></div nataccep></div define>
            <div class="phrgrp"><div class="define"><div class="nataccep">eat small amounts <span class="chn">吃一点点</span chn></div nataccep></div define></div phrgrp>"#;
        assert_eq!(chinese_senses(peck), ["〔鸟〕啄，啄食"]);
        assert_eq!(chinese_senses(LDOCE), ["上前和〔陌生人〕说话"]);
    }

    #[test]
    fn keeps_brackets_paired() {
        assert_eq!(chinese_gloss("oxidize <chn>（使）氧化；（尤指使）生锈</chn>").as_deref(), Some("（使）氧化；（尤指使）生锈"));
    }

    #[test]
    fn finds_bases_of_derived_words() {
        assert_eq!(derivation_bases("justly"), ["just"]);
        assert_eq!(derivation_bases("hastily")[0], "hasty");
        assert_eq!(derivation_bases("swiftness"), ["swift"]);
        assert_eq!(derivation_bases("happiness")[0], "happy");
        assert_eq!(derivation_bases("seamen"), ["seaman"]);
        assert_eq!(derivation_bases("legibly")[0], "legible");
        assert!(derivation_bases("fly").is_empty());
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
