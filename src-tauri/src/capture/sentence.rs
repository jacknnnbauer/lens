//! 断句（SPEC 8.3）：从选区所在段落里切出包含选区的那一句，并给出 UTF-16 高亮区间。
//!
//! 1. 预处理：合并 PDF 的 `-\n` 断词、单个换行换成空格、压缩空白（全程维护原文 → 新文本的偏移映射）
//! 2. `icu_segmenter` 按 UAX #29 切句
//! 3. 后处理：切点落在常见缩写后（Mr. Dr. e.g. U.S. …）时与后句合并
//! 4. 超过 400 字时以选区为中心各取 180 字，两端加 `…`

use icu_segmenter::SentenceSegmenter;
use serde::Serialize;

/// 句子超过这个字数就截取
const MAX_CHARS: usize = 400;
const WINDOW_CHARS: usize = 180;
/// 前后各带几句，供弹窗 Alt+←/→ 扩展语境
const NEIGHBORS: usize = 3;

/// 切点落在这些缩写之后时不算句末（小写比较，含末尾的点）
const ABBREVIATIONS: &[&str] = &[
    "mr.", "mrs.", "ms.", "dr.", "prof.", "st.", "jr.", "sr.", "vs.", "etc.", "e.g.", "i.e.", "u.s.",
    "u.k.", "no.", "fig.", "vol.", "pp.", "approx.", "a.m.", "p.m.", "inc.", "ltd.", "co.", "cf.",
];

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextSentence {
    pub sentence: String,
    /// 选区在 sentence 中的 UTF-16 偏移
    pub hl_start: usize,
    pub hl_end: usize,
    /// 前面的句子（阅读顺序，最近的在最后）
    pub before: Vec<String>,
    /// 后面的句子（阅读顺序，最近的在最前）
    pub after: Vec<String>,
}

/// 预处理后的文本，以及原文每个字节偏移在新文本中的位置。
struct Cleaned {
    text: String,
    map: Vec<usize>,
}

fn clean(src: &str) -> Cleaned {
    let chars: Vec<(usize, char)> = src.char_indices().collect();
    let mut text = String::with_capacity(src.len());
    let mut map = vec![0usize; src.len() + 1];
    let mut i = 0;
    let mut pending_space = false;

    while i < chars.len() {
        let (pos, c) = chars[i];
        // PDF 断词：字母 + '-' + 换行（可带 \r 和缩进）+ 小写字母 → 去掉连字符和换行
        if c == '-' && i > 0 && chars[i - 1].1.is_alphabetic() {
            let mut j = i + 1;
            let mut saw_newline = false;
            while j < chars.len() && chars[j].1.is_whitespace() {
                saw_newline |= chars[j].1 == '\n';
                j += 1;
            }
            if saw_newline && j < chars.len() && chars[j].1.is_lowercase() {
                for k in i..j {
                    map[chars[k].0] = text.len();
                }
                i = j;
                continue;
            }
        }
        if c.is_whitespace() {
            map[pos] = text.len();
            // 连续两个以上换行视为分段，保留为段落分隔；其余空白压成一个空格
            let mut j = i;
            let mut newlines = 0;
            while j < chars.len() && chars[j].1.is_whitespace() {
                newlines += usize::from(chars[j].1 == '\n');
                map[chars[j].0] = text.len();
                j += 1;
            }
            if newlines >= 2 {
                text.push('\n');
                pending_space = false;
            } else if !text.is_empty() {
                pending_space = true;
            }
            i = j;
            continue;
        }
        if pending_space {
            text.push(' ');
            pending_space = false;
        }
        map[pos] = text.len();
        text.push(c);
        i += 1;
    }
    let end = text.len();
    map[src.len()] = end;
    // 被跳过的字符内部字节（多字节字符的中间位置）指向下一个有效位置
    for b in (0..src.len()).rev() {
        if !src.is_char_boundary(b) {
            map[b] = map[b + 1];
        }
    }
    Cleaned { text, map }
}

fn utf16_to_byte(s: &str, u16_off: usize) -> usize {
    let mut units = 0;
    for (b, c) in s.char_indices() {
        if units >= u16_off {
            return b;
        }
        units += c.len_utf16();
    }
    s.len()
}

fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

fn ends_with_abbreviation(seg: &str) -> bool {
    let last = seg.trim_end().rsplit(char::is_whitespace).next().unwrap_or_default();
    let last = last.trim_start_matches(['(', '"', '“', '\'', '‘']).to_lowercase();
    ABBREVIATIONS.contains(&last.as_str())
}

/// 句子边界（字节偏移，含 0 与末尾），合并缩写后的误切。
fn boundaries(text: &str) -> Vec<usize> {
    let raw: Vec<usize> = SentenceSegmenter::new(Default::default()).segment_str(text).collect();
    let mut out = vec![0];
    for w in raw.windows(2) {
        let end = w[1];
        let start = *out.last().unwrap_or(&0);
        let is_last = end == text.len();
        if !is_last && ends_with_abbreviation(&text[start..end]) {
            continue;
        }
        out.push(end);
    }
    if out.last() != Some(&text.len()) {
        out.push(text.len());
    }
    out.dedup();
    out
}

/// 截到选区两侧各 ~WINDOW_CHARS 个字符，切点对齐到单词边界。
/// 返回新句子，以及把原句字节偏移换算到新句的函数所需的 (原句起点, 前缀长度)。
fn window(sentence: &str, sel: (usize, usize)) -> (String, usize, usize) {
    let chars: Vec<(usize, char)> = sentence.char_indices().collect();
    let char_at = |byte: usize| chars.partition_point(|&(b, _)| b < byte);
    let (sc, ec) = (char_at(sel.0), char_at(sel.1));
    let mut from = sc.saturating_sub(WINDOW_CHARS);
    let mut to = (ec + WINDOW_CHARS).min(chars.len());
    if from > 0 {
        while from < sc && !chars[from - 1].1.is_whitespace() {
            from += 1;
        }
    }
    if to < chars.len() {
        while to > ec && !chars[to].1.is_whitespace() {
            to -= 1;
        }
    }
    let byte = |c: usize| chars.get(c).map_or(sentence.len(), |x| x.0);
    let slice = &sentence[byte(from)..byte(to)];
    let origin = byte(from) + (slice.len() - slice.trim_start().len());
    let prefix = if from > 0 { "…" } else { "" };
    let suffix = if to < chars.len() { "…" } else { "" };
    (format!("{prefix}{}{suffix}", slice.trim()), origin, prefix.len())
}

/// 从段落文本中取出包含选区的句子。`sel16` 为选区在段落中的 UTF-16 区间。
pub fn extract(paragraph: &str, sel16: (usize, usize)) -> Option<ContextSentence> {
    let (s16, e16) = (sel16.0.min(sel16.1), sel16.0.max(sel16.1));
    let (sb, eb) = (utf16_to_byte(paragraph, s16), utf16_to_byte(paragraph, e16));
    if sb >= eb {
        return None;
    }
    let cleaned = clean(paragraph);
    let text = &cleaned.text;
    let (cs, ce) = (cleaned.map[sb], cleaned.map[eb]);
    if cs >= ce || text.is_empty() {
        return None;
    }

    let bounds = boundaries(text);
    // 选区所在的第一句和最后一句（选区可能跨句）
    let first = bounds.partition_point(|&b| b <= cs).saturating_sub(1);
    let last = bounds.partition_point(|&b| b < ce).saturating_sub(1).max(first);
    let (start, end) = (bounds[first], bounds[(last + 1).min(bounds.len() - 1)]);

    let raw = &text[start..end];
    let lead = raw.len() - raw.trim_start().len();
    let sentence = raw.trim();
    let (sel_s, sel_e) = (cs - start - lead, (ce - start - lead).min(sentence.len()));

    let (sentence, sel_s, sel_e) = if sentence.chars().count() > MAX_CHARS {
        let (w, origin, prefix) = window(sentence, (sel_s, sel_e));
        // 选区一定落在窗口内（窗口以选区为中心），所以 b >= origin
        let to_new = |b: usize| (b.saturating_sub(origin) + prefix).min(w.len());
        let (a, z) = (to_new(sel_s), to_new(sel_e));
        (w, a, z)
    } else {
        (sentence.to_string(), sel_s, sel_e)
    };

    let seg = |i: usize| text[bounds[i]..bounds[i + 1]].trim().to_string();
    let before = (first.saturating_sub(NEIGHBORS)..first).map(seg).filter(|s| !s.is_empty()).collect();
    let after_end = (last + 1 + NEIGHBORS).min(bounds.len() - 1);
    let after = (last + 1..after_end).map(seg).filter(|s| !s.is_empty()).collect();

    Some(ContextSentence {
        hl_start: utf16_len(&sentence[..sel_s]),
        hl_end: utf16_len(&sentence[..sel_e]),
        sentence,
        before,
        after,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 以选中的子串（第 n 次出现）调用 extract，返回 (句子, 高亮文本)
    fn pick(paragraph: &str, word: &str) -> (String, String) {
        let b = paragraph.find(word).expect("word in paragraph");
        let s16 = utf16_len(&paragraph[..b]);
        let e16 = s16 + utf16_len(word);
        let c = extract(paragraph, (s16, e16)).expect("sentence");
        let hl: String = String::from_utf16_lossy(&c.sentence.encode_utf16().collect::<Vec<_>>()[c.hl_start..c.hl_end]);
        (c.sentence, hl)
    }

    #[test]
    fn picks_the_containing_sentence() {
        let p = "Sensors are cheap. They made tracking ubiquitous in warehouses. Nobody noticed.";
        assert_eq!(pick(p, "ubiquitous"), ("They made tracking ubiquitous in warehouses.".into(), "ubiquitous".into()));
    }

    #[test]
    fn abbreviations_do_not_end_sentences() {
        let p = "We met Dr. Smith and Mr. Jones in the U.S. Army camp. Then we left.";
        assert_eq!(pick(p, "Jones").0, "We met Dr. Smith and Mr. Jones in the U.S. Army camp.");
        let p = "Use a tool, e.g. a hammer. Done.";
        assert_eq!(pick(p, "hammer").0, "Use a tool, e.g. a hammer.");
    }

    #[test]
    fn decimals_and_quotes() {
        let p = "Pi is about 3.14 in value. She said “it is ubiquitous.” Then she left.";
        assert_eq!(pick(p, "3.14").0, "Pi is about 3.14 in value.");
        assert_eq!(pick(p, "ubiquitous").0, "She said “it is ubiquitous.”");
    }

    #[test]
    fn mixed_chinese_and_english() {
        let p = "这是第一句。智能手机如今 ubiquitous，随处可见。最后一句。";
        assert_eq!(pick(p, "ubiquitous"), ("智能手机如今 ubiquitous，随处可见。".into(), "ubiquitous".into()));
    }

    #[test]
    fn pdf_line_breaks_and_hyphenation() {
        let p = "The data is ubiq-\nuitous across\nthe whole system.\n\nNext paragraph here.";
        let b = p.find("ubiq").unwrap();
        let e = p.find("uitous").unwrap() + "uitous".len();
        let c = extract(p, (utf16_len(&p[..b]), utf16_len(&p[..e]))).unwrap();
        assert_eq!(c.sentence, "The data is ubiquitous across the whole system.");
        assert_eq!(&c.sentence[c.hl_start..c.hl_end], "ubiquitous");
        assert_eq!(c.after, ["Next paragraph here."]);
    }

    #[test]
    fn long_sentences_are_windowed_around_the_word() {
        let filler = "word ".repeat(150);
        let p = format!("{filler}target {filler}end.");
        let (s, hl) = pick(&p, "target");
        assert_eq!(hl, "target");
        assert!(s.starts_with('…') && s.ends_with('…'));
        assert!(s.chars().count() <= 2 * WINDOW_CHARS + 10, "{}", s.chars().count());
        assert!(!s.contains("  "));
    }

    #[test]
    fn utf16_offsets_with_astral_chars() {
        let p = "😀 Emoji first. Then 𝒳 marks ubiquitous spots.";
        let c = extract(p, (utf16_len(&p[..p.find("ubiquitous").unwrap()]), utf16_len(&p[..p.find(" spots").unwrap()]))).unwrap();
        let units: Vec<u16> = c.sentence.encode_utf16().collect();
        assert_eq!(String::from_utf16_lossy(&units[c.hl_start..c.hl_end]), "ubiquitous");
    }

    #[test]
    fn neighbors_and_selection_across_sentences() {
        let p = "One. Two. Three. Four. Five. Six. Seven. Eight.";
        let c = extract(p, (utf16_len(&p[..p.find("Five").unwrap()]), utf16_len(&p[..p.find("Five").unwrap() + 4]))).unwrap();
        assert_eq!(c.before, ["Two.", "Three.", "Four."]);
        assert_eq!(c.after, ["Six.", "Seven.", "Eight."]);
        let (s, _) = pick(p, "Two. Three");
        assert_eq!(s, "Two. Three.");
    }

    #[test]
    fn empty_or_reversed_selection() {
        assert!(extract("Hello world.", (3, 3)).is_none());
        assert_eq!(extract("Hello world.", (11, 6)).map(|c| c.sentence), Some("Hello world.".into()));
    }
}
