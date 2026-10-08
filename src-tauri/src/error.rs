use serde::{Serialize, Serializer};

/// 全局错误类型。所有 Tauri 命令返回 `Result<T>`，错误以字符串形式传给前端。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("数据库错误：{0}")]
    Db(#[from] rusqlite::Error),
    #[error("文件读写错误：{0}")]
    Io(#[from] std::io::Error),
    #[error("JSON 解析错误：{0}")]
    Json(#[from] serde_json::Error),
    #[error("索引错误：{0}")]
    Index(#[from] fst::Error),
    #[error("词典格式错误：{0}")]
    InvalidDict(String),
    #[error("暂不支持的词典格式：{0}")]
    UnsupportedFormat(String),
    #[error("该词典已导入：{0}")]
    AlreadyImported(String),
    #[error("数据库版本 {found} 高于本程序支持的 {supported}，请升级 Lens")]
    DbTooNew { found: i32, supported: i32 },
    #[error("{0}")]
    Online(String),
    #[error("内部状态锁已损坏")]
    LockPoisoned,
    #[error("{0}")]
    Other(String),
}

impl Serialize for Error {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
