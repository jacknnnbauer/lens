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
    /// 界面语言：zh | en
    pub language: String,
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
            language: "zh".into(),
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
    /// 有语境句时，查词结果上方显示「当前语境」：这句话里是哪个意思、依据
    pub show_context_sense: bool,
    /// 查词结果里显示「词汇网络」：派生词、同根词、易混词、近义词
    pub show_word_family: bool,
    /// 默认展开哪些卡片：first2 | all | none（卡片已改为始终展开，保留字段以兼容旧设置）
    pub cards_expanded: String,
    /// 每次启动都回到简洁单栏
    pub start_compact: bool,
    /// 左侧导航、右侧栏宽度（px，拖动侧栏边缘调整）
    pub nav_width: u32,
    pub context_width: u32,
    /// 右侧栏显示哪些内容、按什么顺序：coca | vocab | ai | query | dicts | dict:<词典 id>
    pub panel: Vec<PanelItem>,
    /// 卡片墙：各词典卡片按自身高度铺满多列（瀑布流），而不是上下一列
    pub card_wall: bool,
    /// 卡片墙每列最小宽度（px），按窗口宽度自动决定列数
    pub wall_column: u32,
    /// 卡片墙里单张卡片的最大高度（px），超出在卡片内滚动；0 = 不限
    pub wall_max_height: u32,
}

/// 右侧栏的一项
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelItem {
    pub id: String,
    pub show: bool,
}

/// 右侧栏默认内容（新装时的顺序）
pub fn default_panel() -> Vec<PanelItem> {
    ["coca", "vocab", "ai", "query", "dicts"].iter().map(|id| PanelItem { id: (*id).to_string(), show: true }).collect()
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
            show_context_sense: true,
            show_word_family: true,
            cards_expanded: "first2".into(),
            start_compact: true,
            nav_width: 220,
            context_width: 280,
            panel: default_panel(),
            card_wall: false,
            wall_column: 380,
            wall_max_height: 0,
        }
    }
}

/// 生词本。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Wordbook {
    /// 语境卡 / 复习卡背面用哪本词典的释义（词典 id）；空 = 自动（跳过搭配、词频类词典）
    pub card_dict: String,
    /// 主窗口查词（确定的查询）有结果就自动收进生词本
    pub auto_save_lookup: bool,
}

/// 常规：整个应用的行为。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct General {
    /// 启动时检查新版本（每天最多一次）
    pub check_updates: bool,
    /// 上次自动检查的时间（毫秒）
    pub last_update_check: i64,
    /// 点主窗口的关闭按钮时隐藏到托盘（Lens 留在后台，随时可划词查词）；false = 退出 Lens
    pub close_to_tray: bool,
    /// 第一次隐藏到托盘时已经提示过（之后直接隐藏）
    pub tray_hint_shown: bool,
}

impl Default for General {
    fn default() -> Self {
        Self { check_updates: true, last_update_check: 0, close_to_tray: false, tray_hint_shown: false }
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
    /// Lens 窗口内查词（释义、例句、语境句里的词）：dblclick 双击 | click 单击 | drag 划词 | off 关闭
    pub in_app_trigger: String,
    /// 划词查到后自动收进生词本（连同语境句）；默认手动（Ctrl+S / 收藏按钮）
    pub auto_save: bool,
    /// 弹窗大小（逻辑像素）：拖动边缘调整后记住
    pub popup_width: u32,
    pub popup_height: u32,
}

impl Default for Capture {
    fn default() -> Self {
        Self {
            hotkey: "Ctrl+Alt+Space".into(),
            double_copy: true,
            ocr_fallback: false,
            timeout_ms: 150,
            in_app_trigger: "dblclick".into(),
            auto_save: false,
            popup_width: crate::capture::POPUP_W as u32,
            popup_height: crate::capture::POPUP_H as u32,
        }
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
    /// vocabulary.com 真实例句（非官方公开接口，可能随时失效）
    pub vocab_examples: bool,
}

impl Default for Online {
    fn default() -> Self {
        Self { trigger: "localMiss".into(), timeout_ms: 3000, cache_days: 30, vocab_examples: true }
    }
}

/// AI 双语例句（Claude API）。Key 不在这里，在凭据管理器里。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Ai {
    /// claude | deepseek
    pub provider: String,
    /// Claude 的模型
    pub model: String,
    /// DeepSeek 的模型（OpenAI 兼容接口）
    pub deepseek_model: String,
    /// 每次生成几句
    pub count: usize,
    /// 查词后自动生成（否则点按钮再生成）；有缓存时总是直接显示
    pub auto: bool,
}

impl Ai {
    pub fn provider(&self) -> crate::ai::Provider {
        crate::ai::Provider::parse(&self.provider)
    }

    /// 当前服务商用的模型（缓存也按它区分）
    pub fn active_model(&self) -> &str {
        match self.provider() {
            crate::ai::Provider::Claude => &self.model,
            crate::ai::Provider::DeepSeek => &self.deepseek_model,
        }
    }
}

impl Default for Ai {
    fn default() -> Self {
        Self { provider: "claude".into(), model: "claude-opus-4-8".into(), deepseek_model: "deepseek-flash".into(), count: 5, auto: false }
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
    /// 「每个词一个文件」时同时渲染 / 打印的窗口数（WebView2 打印 PDF 是串行的大头，并行几个窗口快得多）
    pub render_workers: usize,
}

impl Default for Batch {
    fn default() -> Self {
        Self { max_words: 2000, lookup_concurrency: 4, render_recycle_every: 150, mem_limit_mb: 800, merge_volume_size: 300, render_workers: 3 }
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
    /// paperWhite | followTheme | inkSaving | theme:<主题 id>
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
    /// 隐藏跳转条与复制按钮（卡片样式与简洁单栏相同）
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
    pub general: General,
    pub wordbook: Wordbook,
    pub appearance: Appearance,
    pub layout: Layout,
    pub immersive: Immersive,
    pub capture: Capture,
    pub online: Online,
    pub copy: Copy,
    pub export: Export,
    pub batch: Batch,
    pub ai: Ai,
    /// 自定义主题（主题库里「复制并编辑」或导入的）
    pub custom_themes: Vec<crate::themes::CustomTheme>,
    pub active_profile: String,
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            general: General::default(),
            wordbook: Wordbook::default(),
            appearance: Appearance::default(),
            layout: Layout::default(),
            immersive: Immersive::default(),
            capture: Capture::default(),
            online: Online::default(),
            copy: Copy::default(),
            export: Export::default(),
            batch: Batch::default(),
            ai: Ai::default(),
            custom_themes: Vec::new(),
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
