//! 生词本导出：Anki 可直接导入的文本文件（带文件头）和 CSV（Excel 能直接打开）。
//! Anki 文本导入的文件头（2.1.55+）：`#separator:tab`、`#html:true`、`#columns:…`、`#tags column:N`，
//! 导入时选一个至少有这几列字段的笔记类型（或新建），字段按列名对应。

use crate::db::history::ContextRow;
use crate::db::words::Word;

/// 语境句里 UTF-16 下标区间对应的三段：前 / 高亮 / 后（下标不合法时整句不高亮）
fn split_highlight(c: &ContextRow) -> (String, String, String) {
    let units: Vec<u16> = c.sentence.encode_utf16().collect();
    let (s, e) = (c.hl_start.max(0) as usize, c.hl_end.max(0) as usize);
    if s >= e || e > units.len() {
        return (c.sentence.clone(), String::new(), String::new());
    }
    let part = |a: usize, b: usize| String::from_utf16_lossy(&units[a..b]);
    (part(0, s), part(s, e), part(e, units.len()))
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Anki 一列里不能有制表符和换行
fn one_line(s: &str) -> String {
    s.split(['\t', '\r', '\n']).filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" ")
}

fn sentence_html(c: &ContextRow) -> String {
    let (a, b, z) = split_highlight(c);
    if b.is_empty() {
        return html_escape(&a);
    }
    format!("{}<b>{}</b>{}", html_escape(&a), html_escape(&b), html_escape(&z))
}

fn source_html(c: &ContextRow) -> String {
    let title = c.window_title.as_deref().unwrap_or_default();
    match c.url.as_deref().filter(|u| u.starts_with("http://") || u.starts_with("https://")) {
        Some(u) => format!("<a href=\"{}\">{}</a>", html_escape(u), html_escape(if title.is_empty() { u } else { title })),
        None => html_escape(title),
    }
}

/// Anki 标签不能有空格
fn anki_tags(w: &Word) -> String {
    let mut tags: Vec<String> = w.tags.iter().map(|t| t.split_whitespace().collect::<Vec<_>>().join("_")).collect();
    tags.push("Lens".into());
    tags.join(" ")
}

/// 毫秒时间戳 → 日期 YYYY-MM-DD（UTC；不为这一处引入日期库）
fn date(ms: i64) -> String {
    let days = ms.div_euclid(86_400_000);
    // 1970-01-01 起的天数 → 公历日期（Howard Hinnant 的 civil_from_days）
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Anki 文本：单词 · 释义 · 语境句 · 来源 · 更多语境 · 标签 · 收藏日期
pub fn anki(words: &[Word]) -> String {
    let mut out = String::from("#separator:tab\n#html:true\n#columns:Word\tDefinition\tSentence\tSource\tMoreContexts\tTags\tAdded\n#tags column:6\n");
    for w in words {
        let first = w.contexts.first();
        let more = w
            .contexts
            .iter()
            .skip(1)
            .map(|c| {
                let src = source_html(&c.context);
                if src.is_empty() {
                    sentence_html(&c.context)
                } else {
                    format!("{} — {}", sentence_html(&c.context), src)
                }
            })
            .collect::<Vec<_>>()
            .join("<br><br>");
        let cols = [
            html_escape(&w.lemma),
            html_escape(w.brief.as_deref().unwrap_or_default()),
            first.map(|c| sentence_html(&c.context)).unwrap_or_default(),
            first.map(|c| source_html(&c.context)).unwrap_or_default(),
            more,
            anki_tags(w),
            date(w.created_at),
        ];
        out.push_str(&cols.iter().map(|c| one_line(c)).collect::<Vec<_>>().join("\t"));
        out.push('\n');
    }
    out
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// CSV（UTF-8 带 BOM，Excel 直接打开不乱码）：每个词一行，语境只取最近一条
pub fn csv(words: &[Word]) -> String {
    let mut out = String::from("\u{feff}单词,释义,语境句,来源,网址,标签,收藏日期,熟悉度,复习次数\n");
    for w in words {
        let c = w.contexts.first().map(|c| &c.context);
        let row = [
            w.lemma.clone(),
            w.brief.clone().unwrap_or_default(),
            c.map(|c| c.sentence.clone()).unwrap_or_default(),
            c.and_then(|c| c.window_title.clone()).unwrap_or_default(),
            c.and_then(|c| c.url.clone()).unwrap_or_default(),
            w.tags.join("; "),
            date(w.created_at),
            w.familiarity.to_string(),
            w.reps.to_string(),
        ];
        out.push_str(&row.iter().map(|f| csv_field(f)).collect::<Vec<_>>().join(","));
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::words::WordContext;

    fn word() -> Word {
        Word {
            id: 1,
            lemma: "recapture".into(),
            brief: Some("to capture again".into()),
            familiarity: 2,
            due_at: None,
            reps: 3,
            created_at: 1_760_000_000_000,
            tags: vec!["news report".into()],
            contexts: vec![
                WordContext {
                    context: ContextRow {
                        id: 1,
                        sentence: "Its forces recaptured \"400\" miles, again.".into(),
                        hl_start: 11,
                        hl_end: 21,
                        app: Some("msedge.exe".into()),
                        window_title: Some("NYT <Opinion>".into()),
                        url: Some("https://nytimes.com/a".into()),
                    },
                    created_at: 0,
                },
                WordContext {
                    context: ContextRow { id: 2, sentence: "A recapture\tlater.".into(), hl_start: 2, hl_end: 11, app: None, window_title: None, url: None },
                    created_at: 0,
                },
            ],
            lookups: 0,
        }
    }

    #[test]
    fn anki_file_has_header_highlight_and_one_line_per_word() {
        let t = anki(&[word()]);
        let lines: Vec<&str> = t.lines().collect();
        assert_eq!(lines[0], "#separator:tab");
        assert!(lines[2].starts_with("#columns:Word\tDefinition"));
        let cols: Vec<&str> = lines[4].split('\t').collect();
        assert_eq!(cols.len(), 7);
        assert_eq!(cols[2], "Its forces <b>recaptured</b> &quot;400&quot; miles, again.");
        assert_eq!(cols[3], "<a href=\"https://nytimes.com/a\">NYT &lt;Opinion&gt;</a>");
        assert_eq!(cols[4], "A <b>recapture</b> later.", "制表符换成空格");
        assert_eq!(cols[5], "news_report Lens");
        assert_eq!(cols[6], "2025-10-09");
    }

    #[test]
    fn csv_quotes_and_has_bom() {
        let t = csv(&[word()]);
        assert!(t.starts_with('\u{feff}'));
        let row = t.lines().nth(1).unwrap_or_default();
        assert!(row.starts_with("recapture,to capture again,\"Its forces recaptured \"\"400\"\" miles, again.\",NYT <Opinion>,"));
        assert!(row.ends_with(",2025-10-09,2,3"));
    }

    #[test]
    fn bad_offsets_do_not_panic() {
        let c = ContextRow { id: 1, sentence: "短句".into(), hl_start: 1, hl_end: 99, app: None, window_title: None, url: None };
        assert_eq!(sentence_html(&c), "短句");
    }
}
