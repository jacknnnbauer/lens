//! 检查新版本：读 GitHub Releases 的最新发布（tag 形如 v0.10.0），和当前版本比较。
//! 只读公开信息，不上传任何数据；仓库没有公开发布时返回「暂无」。

use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

use crate::error::{Error, Result};

/// 发布安装包的公开仓库
pub const REPO: &str = "jacknnnbauer/lens";
const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current: String,
    /// 最新发布的版本；仓库还没有公开发布时为 None
    pub latest: Option<String>,
    pub newer: bool,
    /// 发布页（下载安装包）
    pub url: Option<String>,
    pub notes: Option<String>,
}

/// "v0.10.2" / "0.10.2" → (0, 10, 2)；不是版本号返回 None
pub fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let s = s.trim().trim_start_matches(['v', 'V']);
    let core = s.split(['-', '+']).next()?;
    let mut it = core.split('.').map(|p| p.parse::<u64>().ok());
    let v = (it.next()??, it.next().flatten().unwrap_or(0), it.next().flatten().unwrap_or(0));
    Some(v)
}

pub fn from_release(current: &str, release: &Value) -> UpdateInfo {
    let tag = release.get("tag_name").and_then(Value::as_str).map(str::to_string);
    let newer = match (tag.as_deref().and_then(parse_version), parse_version(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    };
    UpdateInfo {
        current: current.to_string(),
        latest: tag.map(|t| t.trim_start_matches(['v', 'V']).to_string()),
        newer,
        url: release.get("html_url").and_then(Value::as_str).map(str::to_string),
        notes: release.get("body").and_then(Value::as_str).map(|b| b.chars().take(600).collect()),
    }
}

pub async fn check(client: &reqwest::Client) -> Result<UpdateInfo> {
    let current = env!("CARGO_PKG_VERSION");
    let resp = client
        .get(format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .header("accept", "application/vnd.github+json")
        .timeout(TIMEOUT)
        .send()
        .await
        .map_err(|e| Error::Online(crate::tf!("连不上 GitHub：{}", "Could not reach GitHub: {}", e)))?;
    match resp.status().as_u16() {
        // 仓库不公开或还没有发布
        404 => Ok(UpdateInfo { current: current.into(), latest: None, newer: false, url: None, notes: None }),
        s if (200..300).contains(&s) => {
            let v: Value = resp.json().await.map_err(|e| Error::Online(e.to_string()))?;
            Ok(from_release(current, &v))
        }
        403 | 429 => Err(Error::Online(crate::i18n::t("GitHub 暂时限制了请求，稍后再试", "GitHub is rate limiting requests. Try again later").into())),
        s => Err(Error::Online(crate::tf!("检查更新失败（{}）", "Update check failed ({})", s))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn compares_versions() {
        assert_eq!(parse_version("v0.10.2"), Some((0, 10, 2)));
        assert_eq!(parse_version("1.2"), Some((1, 2, 0)));
        assert_eq!(parse_version("v1.0.0-beta.1"), Some((1, 0, 0)));
        assert_eq!(parse_version("latest"), None);
        let r = json!({"tag_name": "v0.10.0", "html_url": "https://github.com/x/y/releases/tag/v0.10.0", "body": "新功能"});
        assert!(from_release("0.9.1", &r).newer, "0.10 > 0.9（按数字比，不按字符串）");
        assert!(!from_release("0.10.0", &r).newer);
        assert_eq!(from_release("0.9.1", &r).latest.as_deref(), Some("0.10.0"));
    }
}
