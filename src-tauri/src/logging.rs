//! 日志：`%APPDATA%/Lens/logs/lens.YYYY-MM-DD.log`，按天滚动，保留 7 天。

use std::path::Path;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{Builder, Rotation};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use crate::error::{Error, Result};

/// 返回的 guard 必须存活到进程结束，否则缓冲中的日志会丢失。
pub fn init(log_dir: &Path) -> Result<WorkerGuard> {
    // 先建目录，否则 tracing-appender 首次清理旧日志时会报找不到路径
    std::fs::create_dir_all(log_dir)?;
    let appender = Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix("lens")
        .filename_suffix("log")
        .max_log_files(7)
        .build(log_dir)
        .map_err(|e| Error::Other(format!("无法创建日志目录：{e}")))?;
    let (writer, guard) = tracing_appender::non_blocking(appender);

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let file_layer = fmt::layer().with_writer(writer).with_ansi(false);
    let stdout_layer = cfg!(debug_assertions).then(fmt::layer);

    tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(stdout_layer)
        .try_init()
        .map_err(|e| Error::Other(format!("日志初始化失败：{e}")))?;
    Ok(guard)
}

/// panic 时写一条 error 日志，再交给默认处理。
pub fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!(panic = %info, "程序发生 panic");
        default_hook(info);
    }));
}
