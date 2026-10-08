//! 自定义主题（SPEC 9.5、画板 09）：存在 settings.json 的 `customThemes`；
//! 导入导出为 `.lens-theme`（JSON）。这里只校验格式（颜色必须是 #RRGGBB），对比度检查在前端编辑器里。

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// 主题变量（与 tokens.css 一致）
pub const VARS: &[&str] = &["bg", "panel", "raised", "line", "text", "sub", "faint", "accent", "accent-ink", "warn"];
const FORMAT: &str = "lens-theme";
const MAX_NAME_CHARS: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomTheme {
    /// `custom-…`
    pub id: String,
    pub name: String,
    /// 暗色系（决定系统控件配色、跟随系统时归入哪一组）
    pub dark: bool,
    pub colors: BTreeMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThemeFile {
    format: String,
    version: u32,
    name: String,
    dark: bool,
    colors: BTreeMap<String, String>,
}

fn is_hex(c: &str) -> bool {
    c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|ch| ch.is_ascii_hexdigit())
}

/// 校验并规范化：名字去空白截断、颜色转大写、必须包含全部变量且不能有多余的键。
pub fn validate(mut t: CustomTheme) -> Result<CustomTheme> {
    t.name = t.name.trim().chars().take(MAX_NAME_CHARS).collect();
    if t.name.is_empty() {
        return Err(Error::Other(crate::i18n::t("主题名不能为空", "The theme needs a name").into()));
    }
    if !t.id.starts_with("custom-") || t.id.len() > 64 || !t.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        t.id = format!("custom-{}", uuid::Uuid::new_v4().simple());
    }
    let mut colors = BTreeMap::new();
    for v in VARS {
        let c = t.colors.get(*v).map(|c| c.trim().to_ascii_uppercase()).unwrap_or_default();
        if !is_hex(&c) {
            return Err(Error::Other(crate::tf!("颜色 {} 无效：需要 #RRGGBB", "Invalid color for {}: expected #RRGGBB", v)));
        }
        colors.insert((*v).to_string(), c);
    }
    t.colors = colors;
    Ok(t)
}

pub fn export(path: &Path, t: &CustomTheme) -> Result<()> {
    let file = ThemeFile { format: FORMAT.into(), version: 1, name: t.name.clone(), dark: t.dark, colors: t.colors.clone() };
    let tmp = path.with_extension("lens-theme.part");
    std::fs::write(&tmp, serde_json::to_vec_pretty(&file)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// 读入 `.lens-theme`；返回一个新 id 的主题（导入两次得到两份，不覆盖已有的）。
pub fn import(path: &Path) -> Result<CustomTheme> {
    let bytes = std::fs::read(path)?;
    if bytes.len() > 64 * 1024 {
        return Err(Error::Other(crate::i18n::t("文件太大，不像主题文件", "The file is too large to be a theme").into()));
    }
    let file: ThemeFile = serde_json::from_slice(&bytes)
        .map_err(|_| Error::Other(crate::i18n::t("不是有效的 .lens-theme 文件", "Not a valid .lens-theme file").into()))?;
    if file.format != FORMAT {
        return Err(Error::Other(crate::i18n::t("不是 Lens 主题文件", "Not a Lens theme file").into()));
    }
    validate(CustomTheme { id: String::new(), name: file.name, dark: file.dark, colors: file.colors })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> CustomTheme {
        CustomTheme {
            id: String::new(),
            name: "  Night Tea  ".into(),
            dark: true,
            colors: VARS.iter().map(|v| ((*v).to_string(), "#a1b2c3".to_string())).collect(),
        }
    }

    #[test]
    fn validates_and_normalizes() -> Result<()> {
        let t = validate(sample())?;
        assert_eq!(t.name, "Night Tea");
        assert!(t.id.starts_with("custom-"));
        assert_eq!(t.colors["accent-ink"], "#A1B2C3");
        let mut bad = sample();
        bad.colors.insert("text".into(), "red".into());
        assert!(validate(bad).is_err());
        let mut missing = sample();
        missing.colors.remove("warn");
        assert!(validate(missing).is_err());
        let mut extra = sample();
        extra.colors.insert("evil".into(), "url(x)".into());
        assert!(!validate(extra)?.colors.contains_key("evil"), "多余的键丢弃");
        Ok(())
    }

    #[test]
    fn export_import_roundtrip() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("night.lens-theme");
        let t = validate(sample())?;
        export(&path, &t)?;
        let back = import(&path)?;
        assert_eq!((back.name.as_str(), back.dark, &back.colors), ("Night Tea", true, &t.colors));
        assert_ne!(back.id, t.id, "导入得到新 id");
        std::fs::write(&path, r#"{"format":"other","version":1,"name":"x","dark":false,"colors":{}}"#)?;
        assert!(import(&path).is_err());
        Ok(())
    }
}
