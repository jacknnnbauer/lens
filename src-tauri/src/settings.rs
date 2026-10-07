//! `%APPDATA%/Lens/settings.json`（SPEC 第 14 节）。
//!
//! 目前只建模 `appearance`；其余分组在各自里程碑接入前原样保留在 `rest` 里，
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub appearance: Appearance,
    pub active_profile: String,
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            appearance: Appearance::default(),
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
        fs::write(&path, r#"{"appearance":{"theme":"light"},"capture":{"hotkey":"Ctrl+Alt+L"}}"#)?;

        let store = SettingsStore::load(path.clone())?;
        assert_eq!(store.get()?.appearance.theme, "light");
        assert_eq!(store.get()?.appearance.accent, "blue");

        store.patch(json!({ "appearance": { "theme": "dark", "accent": "teal" } }))?;
        let reloaded = SettingsStore::load(path)?.get()?;
        assert_eq!(reloaded.appearance.theme, "dark");
        assert_eq!(reloaded.appearance.accent, "teal");
        assert_eq!(reloaded.rest["capture"]["hotkey"], "Ctrl+Alt+L");
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
