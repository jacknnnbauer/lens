mod app_state;
mod capture;
mod clipboard;
mod commands;
mod db;
mod dict;
mod error;
mod export;
mod logging;
mod online;
mod protocol;
mod query;
mod settings;

use tauri::Manager;

use app_state::AppState;

/// 持有日志后台线程的 guard，随 App 一起释放。
struct LogGuard(#[allow(dead_code)] tracing_appender::non_blocking::WorkerGuard);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let root = app.path().data_dir()?.join("Lens");
            std::fs::create_dir_all(&root)?;
            let guard = logging::init(&root.join("logs"))?;
            logging::install_panic_hook();
            app.manage(LogGuard(guard));

            let state = AppState::init(&root)?;
            let schema = state.db.with_conn(db::migrations::current_version)?;
            tracing::info!(root = %state.root.display(), version = env!("CARGO_PKG_VERSION"), schema, "Lens 启动");
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::settings::get_settings,
            commands::settings::set_settings,
        ])
        .run(tauri::generate_context!());

    if let Err(e) = result {
        tracing::error!(error = %e, "Tauri 运行失败");
        eprintln!("Lens 启动失败：{e}");
        std::process::exit(1);
    }
}
