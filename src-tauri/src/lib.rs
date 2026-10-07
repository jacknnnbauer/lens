mod app_state;
mod audio;
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
        .plugin(tauri_plugin_dialog::init())
        .register_asynchronous_uri_scheme_protocol(protocol::SCHEME, |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            let path = request.uri().path().to_string();
            tauri::async_runtime::spawn_blocking(move || responder.respond(protocol::handle(&app, &path)));
        })
        .setup(|app| {
            let root = app.path().data_dir()?.join("Lens");
            std::fs::create_dir_all(&root)?;
            let guard = logging::init(&root.join("logs"))?;
            logging::install_panic_hook();
            app.manage(LogGuard(guard));

            let state = AppState::init(&root)?;
            let schema = state.db.with_conn(db::migrations::current_version)?;
            let dicts = state.dicts.len()?;
            tracing::info!(root = %state.root.display(), version = env!("CARGO_PKG_VERSION"), schema, dicts, "Lens 启动");
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::settings::get_settings,
            commands::settings::set_settings,
            commands::dicts::list_dicts,
            commands::dicts::import_dicts,
            commands::dicts::remove_dict,
            commands::dicts::list_profiles,
            commands::lookup::lookup,
            commands::lookup::cancel_lookup,
            commands::lookup::suggest,
        ])
        .run(tauri::generate_context!());

    if let Err(e) = result {
        tracing::error!(error = %e, "Tauri 运行失败");
        eprintln!("Lens 启动失败：{e}");
        std::process::exit(1);
    }
}
