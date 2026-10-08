mod app_state;
mod audio;
mod batch;
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

use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::ShortcutState;

use app_state::AppState;

/// 持有日志后台线程的 guard，随 App 一起释放。
struct LogGuard(#[allow(dead_code)] tracing_appender::non_blocking::WorkerGuard);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        capture::on_trigger(app, capture::Trigger::Hotkey);
                    }
                })
                .build(),
        )
        .register_asynchronous_uri_scheme_protocol(protocol::SCHEME, |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            let path = request.uri().path().to_string();
            tauri::async_runtime::spawn_blocking(move || responder.respond(protocol::handle(&app, &path)));
        })
        .setup(|app| {
            // LENS_DATA_DIR：开发时与已安装的版本隔开数据（配合 WEBVIEW2_USER_DATA_FOLDER）
            let root = match std::env::var_os("LENS_DATA_DIR") {
                Some(dir) => std::path::PathBuf::from(dir),
                None => app.path().data_dir()?.join("Lens"),
            };
            std::fs::create_dir_all(&root)?;
            let guard = logging::init(&root.join("logs"))?;
            logging::install_panic_hook();
            app.manage(LogGuard(guard));

            let state = AppState::init(&root)?;
            let schema = state.db.with_conn(db::migrations::current_version)?;
            let dicts = state.dicts.len()?;
            tracing::info!(root = %state.root.display(), version = env!("CARGO_PKG_VERSION"), schema, dicts, "Lens 启动");
            let capture_settings = state.settings.get()?.capture;
            app.manage(state);

            // 迷你弹窗：启动时创建、常驻隐藏，用时 show/hide（SPEC 5）
            WebviewWindowBuilder::new(app, capture::POPUP, WebviewUrl::App("index.html".into()))
                .title("词镜 Lens · 查词")
                .inner_size(capture::POPUP_W, capture::POPUP_H)
                .decorations(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .resizable(false)
                .visible(false)
                .focused(false)
                .build()?;

            let service = capture::start(app.handle().clone(), capture_settings.double_copy)?;
            // 快捷键被其他程序占用时不影响启动，状态交给界面提示
            let status = capture::register_hotkey(app.handle(), None, &capture_settings.hotkey);
            if let Ok(mut s) = service.status.lock() {
                *s = capture::CaptureStatus { double_copy: s.double_copy, ..status };
            }
            app.manage(service);
            app.manage(batch::BatchManager::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::settings::get_settings,
            commands::settings::set_settings,
            commands::settings::app_paths,
            commands::dicts::list_dicts,
            commands::dicts::import_dicts,
            commands::dicts::remove_dict,
            commands::dicts::list_profiles,
            commands::lookup::lookup,
            commands::lookup::cancel_lookup,
            commands::lookup::suggest,
            commands::history::record_history,
            commands::history::list_history,
            commands::history::delete_history,
            commands::history::clear_history,
            commands::history::lookup_count,
            commands::words::save_word,
            commands::words::word_id,
            commands::words::list_words,
            commands::words::update_word,
            commands::words::delete_word,
            commands::words::unlink_word_context,
            commands::words::list_tags,
            commands::words::wordbook_stats,
            commands::export::export_prepare,
            commands::export::export_print_pdf,
            commands::export::export_save_html,
            commands::export::latest_context,
            commands::online::list_online_providers,
            commands::online::add_online_dict,
            commands::online::update_online_dict,
            commands::online::set_online_key,
            commands::online::lookup_online,
            commands::online::open_web_dict,
            commands::batch::batch_parse,
            commands::batch::batch_preflight,
            commands::batch::start_batch,
            commands::batch::pause_batch,
            commands::batch::resume_batch,
            commands::batch::cancel_batch,
            commands::batch::retry_failed_batch,
            commands::batch::list_unfinished_batches,
            commands::batch::batch_status,
            commands::batch::batch_items,
            commands::batch::discard_batch,
            commands::batch::batch_render_done,
            commands::batch::batch_window_ready,
            commands::batch::export_print_pdf_outline,
            commands::popup::hide_popup,
            commands::popup::capture_status,
            commands::popup::write_clipboard,
            commands::popup::open_in_main,
        ])
        .run(tauri::generate_context!());

    if let Err(e) = result {
        tracing::error!(error = %e, "Tauri 运行失败");
        eprintln!("Lens 启动失败：{e}");
        std::process::exit(1);
    }
}
