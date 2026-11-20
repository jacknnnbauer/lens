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
    /// 阅读正文字号（px）
    pub reader_font_size: u32,
    /// 主窗口缩放（百分比，80–150）
    pub ui_zoom: u32,
    /// 界面语言：zh | en
    pub language: String,
    /// 运行词典自带的脚本（放在隔离的沙盒里）
    pub dict_scripts: bool,
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
            reader_font_size: 17,
            ui_zoom: 100,
            gloss_font: "serif".into(),
            language: "zh".into(),
            dict_scripts: true,
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
    /// 词汇网络的显示方式：graph（放射图）| table（表格：词性 + 中文释义）
    pub word_family_view: String,
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
            word_family_view: "graph".into(),
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Wordbook {
    /// 语境卡 / 复习卡背面用哪本词典的释义（词典 id）；空 = 自动（跳过搭配、词频类词典）
    pub card_dict: String,
    /// 主窗口查词（确定的查询）有结果就自动收进生词本
    pub auto_save_lookup: bool,
    /// 词典里查不到（如自由词组 first year）也允许收藏，释义留给用户在生词本里自己写
    pub save_unmatched: bool,
}

impl Default for Wordbook {
    fn default() -> Self {
        Self { card_dict: String::new(), auto_save_lookup: false, save_unmatched: true }
    }
}

/// 全书预习包
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Study {
    /// 读前导读里放情节梗概（会剧透结局）
    pub spoilers: bool,
    /// 读前导读用哪种语言的维基百科：zh = 有中文条目就用中文，en = 英文
    pub guide_lang: String,
}

impl Default for Study {
    fn default() -> Self {
        Self { spoilers: false, guide_lang: "zh".into() }
    }
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
    /// 第一次启动已经过了：全新安装第一次启动时展开左侧导航，不按「启动回到简洁单栏」收起。
    /// 旧版本的设置文件里没有这一项，算已经过了
    #[serde(default = "yes")]
    pub intro_done: bool,
    /// 用户删除过内置词典：以后不再自动加回
    pub builtin_dict_removed: bool,
    /// 看过（点了「知道了」或用过一次）的一次性提示：nav | han-char | toc
    pub hints_seen: Vec<String>,
}

fn yes() -> bool {
    true
}

impl Default for General {
    fn default() -> Self {
        Self { check_updates: true, last_update_check: 0, close_to_tray: false, tray_hint_shown: false, intro_done: false, builtin_dict_removed: false, hints_seen: Vec::new() }
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
    /// 查词小窗用的词典：空 = 跟随当前配置组；"custom" = 用 popup_dicts；其余是场景配置组 id（组不存在时按跟随处理）
    pub popup_profile: String,
    /// popup_profile 为 "custom" 时小窗查的词典 id（顺序按词典库）
    pub popup_dicts: Vec<String>,
    /// 查词小窗是否显示「当前语境」；没设过（None）跟随 layout.show_context_sense
    #[serde(skip_serializing_if = "Option::is_none")]
    pub popup_context_sense: Option<bool>,
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
            popup_profile: String::new(),
            popup_dicts: Vec::new(),
            popup_context_sense: None,
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

/// AI 双语例句（DeepSeek）。Key 不在这里，在凭据管理器里。
/// 旧版设置文件里的 provider、model（Claude 用的）读取时直接忽略。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Ai {
    /// DeepSeek 的模型（缓存也按它区分）
    pub deepseek_model: String,
    /// 每次生成几句
    pub count: usize,
    /// 查词后自动生成（否则点按钮再生成）；有缓存时总是直接显示
    pub auto: bool,
}

impl Default for Ai {
    fn default() -> Self {
        Self { deepseek_model: "deepseek-flash".into(), count: 5, auto: false }
    }
}

/// 词汇量和阅读的生词门槛。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Vocab {
    /// 阅读的生词门槛；0 = 还没设置过
    pub level: u32,
    /// 门槛跟随词汇量（手动选过别的档位后为 false）
    pub follow: bool,
    /// 当前词汇量；0 = 还没测过
    pub estimate: u32,
}

impl Default for Vocab {
    fn default() -> Self {
        Self { level: 0, follow: true, estimate: 0 }
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

/// 英文怎么说的对象（设置 › AI）：名字和一行描述
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Audience {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub note: String,
}

/// 英文怎么说：存好的对象、上次选的哪个（空 = 不指定）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Expression {
    pub audiences: Vec<Audience>,
    pub last_audience: String,
}

impl Default for Expression {
    fn default() -> Self {
        let a = |id: &str, name: &str, note: &str| Audience { id: id.into(), name: name.into(), note: note.into() };
        Self {
            audiences: vec![
                a("jordan-sub", "约旦分包商", "英语一般，阿拉伯语母语，合作方；句子要短，少用习语和缩写"),
                a("uk-employer", "英国业主代表", "英语母语，正式，甲方；用英式拼写和说法（programme、site）"),
            ],
            last_audience: String::new(),
        }
    }
}

/// 阅读页点词时用的词典（设置 › 场景配置组）
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Reading {
    /// 空 = 跟随当前配置组；"custom" = 用 dicts；其余是场景配置组 id（组不存在时按跟随处理）
    pub profile: String,
    /// profile 为 "custom" 时阅读页查的词典 id（顺序按词典库）
    pub dicts: Vec<String>,
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
    pub study: Study,
    pub vocab: Vocab,
    pub reading: Reading,
    pub expression: Expression,
    /// 自定义主题（主题库里「复制并编辑」或导入的）
    pub custom_themes: Vec<crate::themes::CustomTheme>,
    pub active_profile: String,
    /// 应用内快捷键：只存改过的（动作 id → 组合键，如 save → Ctrl+Shift+S），没改的用前端默认值
    pub keys: std::collections::BTreeMap<String, String>,
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
            study: Study::default(),
            vocab: Vocab::default(),
            reading: Reading::default(),
            expression: Expression::default(),
            custom_themes: Vec::new(),
            active_profile: "reading".into(),
            keys: Default::default(),
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

    #[test]
    fn old_claude_ai_settings_still_load() {
        // 旧版存过 Claude 的 provider、model：读得进来，其余设置不丢
        let a: Ai = serde_json::from_str(r#"{"provider":"claude","model":"claude-opus-4-8","deepseekModel":"deepseek-v4-pro","count":3,"auto":true}"#).unwrap();
        assert_eq!(a, Ai { deepseek_model: "deepseek-v4-pro".into(), count: 3, auto: true });
    }
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
    fn expression_defaults_have_two_presets() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("settings.json");
        fs::write(&path, r#"{"layout":{"preset":"compact"}}"#)?;
        let e = SettingsStore::load(path)?.get()?.expression;
        assert_eq!(e.audiences.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["约旦分包商", "英国业主代表"]);
        assert!(e.last_audience.is_empty());
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
        let a = store.get()?.appearance;
        assert_eq!((a.reader_font_size, a.ui_zoom), (17, 100), "正文字号、界面缩放的默认值");
        let l = store.patch(json!({ "layout": { "showNav": true } }))?.layout;
        assert_eq!((l.preset.as_str(), l.show_nav), ("three-column", true));
        Ok(())
    }

    #[test]
    fn only_a_fresh_install_counts_as_first_run() -> Result<()> {
        let dir = tempfile::tempdir()?;
        // 没有设置文件：全新安装
        let fresh = SettingsStore::load(dir.path().join("none.json"))?.get()?.general;
        assert!(!fresh.intro_done && fresh.hints_seen.is_empty());
        // 旧版本写的设置文件没有这一项：不算第一次
        let path = dir.path().join("settings.json");
        fs::write(&path, r#"{"general":{"checkUpdates":true}}"#)?;
        assert!(SettingsStore::load(path)?.get()?.general.intro_done);
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
