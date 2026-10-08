//! `%APPDATA%/Lens/settings.json`（SPEC 第 14 节）。
//!
//! 目前只建模 `appearance`、`layout`、`immersive`、`capture`、`online`、`copy`、`export`、`batch`；其余分组在各自里程碑接入前原样保留在 `rest` 里，
//! 这样用户手改的内容不会在保存时丢失。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Appearance {
    pub theme: String,
    pub follow_system: bool,
    pub light_theme: String,
    pub dark_theme: String,
    pub accent: String,
    pub gloss_font_size: u32,
    pub gloss_font: String,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: "paper".into(),
            follow_system: false,
            light_theme: "paper".into(),
            dark_theme: "dark".into(),
            accent: "blue".into(),
            gloss_font_size: 17,
            gloss_font: "serif".into(),
        }
    }
}

/// 布局（SPEC 9.2）。preset 决定默认显示哪些区域，各区域开关可单独改。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Layout {
    /// compact | two-column | three-column
    pub preset: String,
    pub show_nav: bool,
    pub show_sense_atlas: bool,
    pub show_context: bool,
    pub show_status_bar: bool,
    pub show_jump_bar: bool,
    /// 默认展开哪些卡片：first2 | all | none
    pub cards_expanded: String,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            preset: "compact".into(),
            show_nav: false,
            show_sense_atlas: false,
            show_context: false,
            show_status_bar: false,
            show_jump_bar: true,
            cards_expanded: "first2".into(),
        }
    }
}

/// 取词（SPEC 8.1、14）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Capture {
    /// 全局快捷键，避开 Alt+Space（系统菜单）与 Ctrl+Space（输入法）
    pub hotkey: String,
    /// 是否启用 Ctrl+C+C
    pub double_copy: bool,
    pub ocr_fallback: bool,
    /// 取词超时，超时先弹窗
    pub timeout_ms: u64,
}

impl Default for Capture {
    fn default() -> Self {
        Self { hotkey: "Ctrl+Alt+Space".into(), double_copy: true, ocr_fallback: false, timeout_ms: 150 }
    }
}

/// 在线词典（SPEC 13、14）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Online {
    /// 默认查询时机：always | localMiss | manual（每部词典可单独覆盖）
    pub trigger: String,
    pub timeout_ms: u64,
    /// 缓存天数：0 = 不缓存，-1 = 永久
    pub cache_days: i64,
}

impl Default for Online {
    fn default() -> Self {
        Self { trigger: "localMiss".into(), timeout_ms: 3000, cache_days: 30 }
    }
}

/// 批量导出（SPEC 12、14）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Batch {
    /// 单批上限
    pub max_words: usize,
    pub lookup_concurrency: usize,
    /// 渲染窗口每处理这么多个词就重建一次
    pub render_recycle_every: usize,
    /// 进程（含 WebView2 子进程）内存超过这个值也重建渲染窗口
    pub mem_limit_mb: u64,
    /// 合并模式每卷词数
    pub merge_volume_size: usize,
}

impl Default for Batch {
    fn default() -> Self {
        Self { max_words: 2000, lookup_concurrency: 4, render_recycle_every: 150, mem_limit_mb: 800, merge_volume_size: 300 }
    }
}

/// 复制（SPEC 10、14）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Copy {
    /// md | html | text
    pub default_format: String,
    pub append_source: bool,
    pub include_examples: bool,
    pub inline_html_styles: bool,
    pub md_images_as_links: bool,
}

impl Default for Copy {
    fn default() -> Self {
        Self {
            default_format: "md".into(),
            append_source: true,
            include_examples: true,
            inline_html_styles: true,
            md_images_as_links: true,
        }
    }
}

/// 单词导出（SPEC 11、14）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Export {
    /// cards | document
    pub layout: String,
    /// paperWhite | followTheme | inkSaving
    pub style: String,
    /// a4 | letter
    pub paper: String,
    /// 上次导出的目录
    pub dir: Option<String>,
}

impl Default for Export {
    fn default() -> Self {
        Self { layout: "cards".into(), style: "paperWhite".into(), paper: "a4".into(), dir: None }
    }
}

/// 沉浸模式（SPEC 9.2、14）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Immersive {
    pub hide_title_bar: bool,
    pub hide_status_bar: bool,
    pub hide_sidebars: bool,
    /// 隐藏跳转条与复制按钮，卡片去边框只留小标题
    pub hide_card_chrome: bool,
    pub borderless: bool,
    /// 鼠标在顶部停留时浮现工具栏
    pub reveal_on_hover: bool,
    pub fullscreen: bool,
    /// 正文宽度：600 | 720 | 900
    pub width: u32,
    /// 沉浸时单独使用的主题；None = 跟随当前主题
    pub theme: Option<String>,
    pub on_startup: bool,
}

impl Default for Immersive {
    fn default() -> Self {
        Self {
            hide_title_bar: true,
            hide_status_bar: true,
            hide_sidebars: true,
            hide_card_chrome: true,
            borderless: true,
            reveal_on_hover: true,
            fullscreen: false,
            width: 720,
            theme: None,
            on_startup: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub appearance: Appearance,
    pub layout: Layout,
    pub immersive: Immersive,
    pub capture: Capture,
    pub online: Online,
    pub copy: Copy,
    pub export: Export,
    pub batch: Batch,
    pub active_profile: String,
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            appearance: Appearance::default(),
            layout: Layout::default(),
            immersive: Immersive::default(),
            capture: Capture::default(),
            online: Online::default(),
            copy: Copy::default(),
            export: Export::default(),
            batch: Batch::default(),
            active_profile: "reading".into(),
            rest: Map::new(),
        }
    }
}

pub struct SettingsStore {
    path: PathBuf,
    current: Mutex<Settings>,
}

impl SettingsStore {
    /// 读取设置；文件不存在用默认值；文件损坏则改名为 `settings.json.bad` 后用默认值。
    pub fn load(path: PathBuf) -> Result<Self> {
        let current = match fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str(&text) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(error = %e, "settings.json 无法解析，已备份并使用默认设置");
                    fs::rename(&path, path.with_extension("json.bad"))?;
                    Settings::default()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Settings::default(),
            Err(e) => return Err(e.into()),
        };
        Ok(Self { path, current: Mutex::new(current) })
    }

    pub fn get(&self) -> Result<Settings> {
        Ok(self.current.lock().map_err(|_| Error::LockPoisoned)?.clone())
    }

    /// 以 JSON Merge Patch（RFC 7396）方式合并后原子写盘，返回合并后的完整设置。
    pub fn patch(&self, patch: Value) -> Result<Settings> {
        let mut guard = self.current.lock().map_err(|_| Error::LockPoisoned)?;
        let mut merged = serde_json::to_value(&*guard)?;
        merge_patch(&mut merged, patch);
        let next: Settings = serde_json::from_value(merged)?;
        write_atomic(&self.path, &serde_json::to_vec_pretty(&next)?)?;
        *guard = next.clone();
        Ok(next)
    }
}

fn merge_patch(target: &mut Value, patch: Value) {
    match patch {
        Value::Object(patch) => {
            if !target.is_object() {
                *target = Value::Object(Map::new());
            }
            if let Value::Object(t) = target {
                for (k, v) in patch {
                    if v.is_null() {
                        t.remove(&k);
                    } else {
                        merge_patch(t.entry(k).or_insert(Value::Null), v);
                    }
                }
            }
        }
        other => *target = other,
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn patch_persists_and_keeps_unknown_sections() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("settings.json");
        fs::write(&path, r#"{"appearance":{"theme":"light"},"future":{"x":500}}"#)?;

        let store = SettingsStore::load(path.clone())?;
        assert_eq!(store.get()?.appearance.theme, "light");
        assert_eq!(store.get()?.appearance.accent, "blue");

        store.patch(json!({ "appearance": { "theme": "dark", "accent": "teal" } }))?;
        let reloaded = SettingsStore::load(path)?.get()?;
        assert_eq!(reloaded.appearance.theme, "dark");
        assert_eq!(reloaded.appearance.accent, "teal");
        assert_eq!(reloaded.rest["future"]["x"], 500);
        Ok(())
    }

    #[test]
    fn layout_defaults_follow_spec_and_merge() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("settings.json");
        fs::write(&path, r#"{"layout":{"preset":"three-column"}}"#)?;
        let store = SettingsStore::load(path)?;
        let l = store.get()?.layout;
        assert_eq!(l.preset, "three-column");
        assert!(l.show_jump_bar && !l.show_nav, "未写的字段取默认值");
        let l = store.patch(json!({ "layout": { "showNav": true } }))?.layout;
        assert_eq!((l.preset.as_str(), l.show_nav), ("three-column", true));
        Ok(())
    }

    #[test]
    fn immersive_theme_can_be_cleared() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let store = SettingsStore::load(dir.path().join("settings.json"))?;
        let i = store.patch(json!({ "immersive": { "theme": "dark", "width": 900 } }))?.immersive;
        assert_eq!((i.theme.as_deref(), i.width), (Some("dark"), 900));
        // null 在 Merge Patch 里表示删除，回到默认「跟随当前主题」
        let i = store.patch(json!({ "immersive": { "theme": null } }))?.immersive;
        assert_eq!((i.theme, i.width), (None, 900));
        Ok(())
    }

    #[test]
    fn corrupt_file_falls_back_to_defaults() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("settings.json");
        fs::write(&path, "{ not json")?;
        let store = SettingsStore::load(path.clone())?;
        assert_eq!(store.get()?, Settings::default());
        assert!(path.with_extension("json.bad").exists());
        Ok(())
    }

    #[test]
    fn invalid_patch_leaves_settings_untouched() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let store = SettingsStore::load(dir.path().join("settings.json"))?;
        assert!(store.patch(json!({ "appearance": { "glossFontSize": "big" } })).is_err());
        assert_eq!(store.get()?, Settings::default());
        Ok(())
    }
}
