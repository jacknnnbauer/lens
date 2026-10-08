//! 界面语言（设置 › 外观 › 界面语言）。只翻译会显示给用户的提示（在线词典错误、批量导出提示、
//! 总目录等）；词典文件解析这类底层错误仍是中文。启动时与改设置时同步。

use std::sync::atomic::{AtomicBool, Ordering};

static EN: AtomicBool = AtomicBool::new(false);

pub fn set_language(lang: &str) {
    EN.store(lang == "en", Ordering::Relaxed);
}

pub fn en() -> bool {
    EN.load(Ordering::Relaxed)
}

/// 按当前语言二选一。
pub fn t(zh: &'static str, en: &'static str) -> &'static str {
    if self::en() {
        en
    } else {
        zh
    }
}

/// 带参数的提示：`tf!("中文 {}", "English {}", x)`（只用位置参数）。
#[macro_export]
macro_rules! tf {
    ($zh:literal, $en:literal $(, $arg:expr)* $(,)?) => {
        if $crate::i18n::en() { format!($en $(, $arg)*) } else { format!($zh $(, $arg)*) }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    // 不在测试里切到英文：语言是全局状态，会影响并行跑的其他测试
    #[test]
    fn chinese_by_default() {
        assert_eq!(t("请求超时", "Request timed out"), "请求超时");
        assert_eq!(crate::tf!("超时（{} 秒）", "Timed out ({} s)", 3), "超时（3 秒）");
    }
}
