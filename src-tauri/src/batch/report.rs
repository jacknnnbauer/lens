//! 批量导出完成后的总目录 `index.html` 与失败清单 `failed.csv`（SPEC 12.3）。

use std::path::Path;

use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};

use super::BatchRequest;
use crate::db::batch::{ItemRow, DONE, FAILED};
use crate::error::Result;

/// 文件名放进 href 时要转义的字符
const HREF: &AsciiSet = &CONTROLS.add(b' ').add(b'"').add(b'#').add(b'%').add(b'<').add(b'>').add(b'?').add(b'`').add(b'{').add(b'}');

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

pub fn index_html(req: &BatchRequest, items: &[ItemRow]) -> String {
    let done: Vec<&ItemRow> = items.iter().filter(|i| i.state == DONE).collect();
    let failed: Vec<&ItemRow> = items.iter().filter(|i| i.state == FAILED).collect();
    let mut rows = String::new();
    for i in &done {
        let links: Vec<String> = i
            .files
            .iter()
            .map(|f| {
                let ext = f.rsplit('.').next().unwrap_or_default().to_uppercase();
                format!("<a href=\"{}\">{}</a>", utf8_percent_encode(f, HREF), esc(&ext))
            })
            .collect();
        rows.push_str(&format!(
            "<tr><td class=\"w\">{}</td><td>{}</td><td class=\"n\">{}</td></tr>\n",
            esc(&i.lemma),
            links.join(" · "),
            esc(i.note.as_deref().unwrap_or_default())
        ));
    }
    let mut failed_rows = String::new();
    for i in &failed {
        failed_rows.push_str(&format!(
            "<tr><td class=\"w\">{}</td><td class=\"n\">{}</td></tr>\n",
            esc(&i.lemma),
            esc(i.note.as_deref().unwrap_or_default())
        ));
    }
    let failed_section = if failed.is_empty() {
        String::new()
    } else {
        format!("<h2>未能导出（{}）</h2><p class=\"n\">详见 failed.csv</p><table>{failed_rows}</table>", failed.len())
    };
    format!(
        r#"<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title} · Lens 词镜</title>
<style>
:root {{ --bg:#F7F6F3; --panel:#FFFFFF; --line:#E2E0DA; --text:#1A1B1E; --sub:#4E5258; --accent:#2F5FD0; --warn:#A64F0C; }}
@media (prefers-color-scheme: dark) {{ :root {{ --bg:#141518; --panel:#1B1D21; --line:#2C3036; --text:#ECEAE4; --sub:#A3A7AF; --accent:#8AB4F8; --warn:#F2A65A; }} }}
body {{ margin:0; background:var(--bg); color:var(--text); font:14px/1.6 "IBM Plex Sans","Microsoft YaHei UI",sans-serif; }}
main {{ max-width:860px; margin:0 auto; padding:40px 20px 80px; }}
h1 {{ margin:0 0 6px; font:600 28px/1.2 "Source Serif 4",Georgia,serif; }}
h2 {{ margin:36px 0 10px; font-size:17px; }}
p {{ margin:0; color:var(--sub); }}
table {{ width:100%; border-collapse:collapse; background:var(--panel); border:1px solid var(--line); border-radius:10px; }}
td {{ padding:7px 12px; border-top:1px solid var(--line); vertical-align:top; }}
tr:first-child td {{ border-top:0; }}
.w {{ width:30%; font-family:"Source Serif 4",Georgia,serif; font-size:16px; }}
.n {{ color:var(--sub); font-size:12px; }}
a {{ color:var(--accent); }}
h2 + p.n {{ margin-bottom:8px; color:var(--warn); }}
footer {{ margin-top:36px; font-size:12px; color:var(--sub); }}
</style>
</head>
<body>
<main>
<h1>{title}</h1>
<p>共 {total} 个词 · 已导出 {done} · 未能导出 {failed}</p>
<h2>已导出</h2>
<table>
{rows}</table>
{failed_section}
<footer>由 Lens 词镜导出。释义内容版权归各词典所有，仅供个人学习使用。</footer>
</main>
</body>
</html>
"#,
        title = esc(&req.label),
        total = items.len(),
        done = done.len(),
        failed = failed.len(),
    )
}

pub fn failed_csv(items: &[ItemRow]) -> String {
    // 带 BOM，Excel 打开不乱码
    let mut s = String::from("\u{feff}词,原因\r\n");
    for i in items.iter().filter(|i| i.state == FAILED) {
        s.push_str(&format!("{},{}\r\n", csv_field(&i.lemma), csv_field(i.note.as_deref().unwrap_or_default())));
    }
    s
}

pub fn write(out_dir: &Path, req: &BatchRequest, items: &[ItemRow]) -> Result<()> {
    std::fs::write(out_dir.join("index.html"), index_html(req, items))?;
    let failed = out_dir.join("failed.csv");
    if items.iter().any(|i| i.state == FAILED) {
        std::fs::write(failed, failed_csv(items))?;
    } else if failed.exists() {
        // 重试后全部成功：删掉旧的失败清单
        std::fs::remove_file(failed)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::Paper;

    fn req() -> BatchRequest {
        BatchRequest {
            source: "paste".into(),
            label: "粘贴词表 · 2026-10-08".into(),
            words: vec![],
            pdf: true,
            html: true,
            merge: false,
            layout: "cards".into(),
            style: "paperWhite".into(),
            paper: Paper::A4,
            offline_only: false,
            out_dir: String::new(),
            options: serde_json::json!({}),
            theme: "paper".into(),
            accent: "blue".into(),
            profile: String::new(),
        }
    }

    fn item(seq: i64, lemma: &str, state: &str, note: Option<&str>, files: &[&str]) -> ItemRow {
        ItemRow { seq, lemma: lemma.into(), state: state.into(), note: note.map(str::to_string), files: files.iter().map(|f| f.to_string()).collect() }
    }

    #[test]
    fn index_links_and_failed_list() {
        let items = vec![
            item(0, "take into account", DONE, None, &["take into account.pdf", "take into account.html"]),
            item(1, "résumé", DONE, Some("文件名已规范为 resume"), &["resume.pdf"]),
            item(2, "qwertyz", FAILED, Some("所有词典均无结果, 拼写建议：qwerty"), &[]),
        ];
        let html = index_html(&req(), &items);
        assert!(html.contains("href=\"take%20into%20account.pdf\""));
        assert!(html.contains("共 3 个词 · 已导出 2 · 未能导出 1"));
        assert!(html.contains("未能导出（1）"));
        let csv = failed_csv(&items);
        assert!(csv.starts_with('\u{feff}'));
        assert!(csv.contains("qwertyz,\"所有词典均无结果, 拼写建议：qwerty\"\r\n"));
    }

    #[test]
    fn write_removes_stale_failed_csv() -> Result<()> {
        let dir = tempfile::tempdir()?;
        std::fs::write(dir.path().join("failed.csv"), "old")?;
        write(dir.path(), &req(), &[item(0, "a", DONE, None, &["a.pdf"])])?;
        assert!(dir.path().join("index.html").exists());
        assert!(!dir.path().join("failed.csv").exists());
        Ok(())
    }
}
