//! 从本地词典里找一个词的发音文件（Anki 牌组包用）：词条里的 `sound://…` 链接，
//! 按英音 / 美音偏好挑一个，从 mdd 读出来；Speex（.spx）转成 WAV（Anki 不一定能放 spx）。

use crate::anki_pkg::Media;
use crate::dict::{EntryBody, LoadedDict};

/// 词条 HTML 里的 `sound://` 路径（在线词典的 `sound://remote/…` 不算）
pub fn sound_paths(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    for part in html.split("sound://").skip(1) {
        let path: String = part.chars().take_while(|c| !matches!(c, '"' | '\'' | '>' | '<' | ' ' | ')')).collect();
        let lower = path.to_lowercase();
        if !path.is_empty() && !lower.starts_with("remote/") && [".mp3", ".spx", ".wav", ".ogg", ".m4a"].iter().any(|e| lower.ends_with(e)) && !out.contains(&path) {
            out.push(path);
        }
    }
    out
}

/// 文件名像哪种口音：2 = 和偏好一致，1 = 看不出，0 = 另一种
fn accent_score(path: &str, prefer: &str) -> u8 {
    let p = path.to_lowercase().replace('\\', "/");
    let name = p.rsplit('/').next().unwrap_or_default();
    let us = ["_us", "us_", "/us/", "ame", "_a.", "-us", "us/"].iter().any(|m| p.contains(m) || name.starts_with("us"));
    let uk = ["_uk", "uk_", "/uk/", "_gb", "gb_", "bre", "_e.", "-uk", "_br", "uk/"].iter().any(|m| p.contains(m) || name.starts_with("uk") || name.starts_with("gb"));
    let (mine, other) = if prefer == "uk" { (uk, us) } else { (us, uk) };
    match (mine, other) {
        (true, false) => 2,
        (false, true) => 0,
        _ => 1,
    }
}

fn safe_name(word: &str) -> String {
    word.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c.to_ascii_lowercase() } else { '_' }).collect()
}

/// 按词典顺序找第一部有发音的，挑口音最合适的那个
pub fn find(dicts: &[LoadedDict], word: &str, prefer: &str) -> Option<Media> {
    for d in dicts {
        let Ok(entries) = d.source.lookup(word) else { continue };
        let mut paths: Vec<String> = Vec::new();
        for e in &entries {
            if let EntryBody::Html(h) = &e.body {
                for p in sound_paths(h) {
                    if !paths.contains(&p) {
                        paths.push(p);
                    }
                }
            }
        }
        // 稳定排序：同分时保持词条里的先后
        paths.sort_by_key(|p| std::cmp::Reverse(accent_score(p, prefer)));
        for p in paths {
            let Ok(Some(data)) = d.source.resource(&p) else { continue };
            let ext = p.rsplit('.').next().unwrap_or("mp3").to_lowercase();
            let (data, ext) = if ext == "spx" {
                match crate::audio::spx_to_wav(&data) {
                    Ok(w) => (w, "wav".to_string()),
                    Err(e) => {
                        tracing::debug!(path = %p, error = %e, "spx 转码失败");
                        continue;
                    }
                }
            } else {
                (data, ext)
            };
            return Some(Media { name: format!("lens-{}.{ext}", safe_name(word)), data });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_sound_links_and_skips_remote() {
        let h = r#"<a href="sound://us/rust__us_1.mp3">US</a><a href='sound://uk/rust__gb_1.spx'>UK</a><a href="sound://remote/x.com/a.mp3"></a><a href="sound://us/rust__us_1.mp3"></a>"#;
        assert_eq!(sound_paths(h), vec!["us/rust__us_1.mp3", "uk/rust__gb_1.spx"]);
    }

    #[test]
    fn prefers_requested_accent() {
        assert_eq!(accent_score("us/rust__us_1.mp3", "us"), 2);
        assert_eq!(accent_score("uk/rust__gb_1.spx", "us"), 0);
        assert_eq!(accent_score("uk/rust__gb_1.spx", "uk"), 2);
        assert_eq!(accent_score("rust.mp3", "uk"), 1);
        assert_eq!(safe_name("take off"), "take_off");
    }
}
