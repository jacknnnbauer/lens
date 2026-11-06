//! 启动失败时的系统对话框。发布版没有控制台，不弹窗的话用户只看到「双击没反应」。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

static LOG_DIR: OnceLock<PathBuf> = OnceLock::new();
static STARTED: AtomicBool = AtomicBool::new(false);

/// 日志目录建好后记下，失败提示里告诉用户去哪看
pub fn set_log_dir(dir: PathBuf) {
    let _ = LOG_DIR.set(dir);
}

/// 主窗口已经显示：之后的问题不再按「启动失败」弹窗
pub fn mark_started() {
    STARTED.store(true, Ordering::Relaxed);
}

/// 启动阶段主线程 panic（Tauri 的 setup 出错也是这样报出来的）时弹窗说明；其他线程、启动之后的 panic 只写日志
pub fn install_panic_dialog() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        previous(info);
        if !STARTED.load(Ordering::Relaxed) && std::thread::current().name() == Some("main") {
            let reason = info.payload().downcast_ref::<String>().map(String::as_str).or_else(|| info.payload().downcast_ref::<&str>().copied()).unwrap_or("panic");
            startup_failed(reason);
            // 发布版 panic = "unwind"：不退出的话，关掉对话框后进程可能还留着，再双击会被单实例转给这个残留进程
            std::process::exit(1);
        }
    }));
}

pub fn startup_failed(reason: &str) {
    // Tauri 把 setup 的错误包成「Failed to setup app: error encountered during setup hook: …」
    let reason = reason.rsplit("setup hook: ").next().unwrap_or(reason).trim();
    let mut message = crate::tf!("Lens 启动失败：{}", "Lens failed to start: {}", reason);
    if let Some(dir) = LOG_DIR.get() {
        message.push_str(&crate::tf!("\n\n日志在：{}", "\n\nLogs: {}", dir.display()));
    }
    show_error(&message);
}

#[cfg(windows)]
pub fn show_error(message: &str) {
    use windows::core::HSTRING;
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
    // SAFETY：两个字符串在调用期间都有效，没有父窗口
    unsafe {
        MessageBoxW(None, &HSTRING::from(message), &HSTRING::from("Lens"), MB_OK | MB_ICONERROR);
    }
}

#[cfg(not(windows))]
pub fn show_error(message: &str) {
    eprintln!("{message}");
}
