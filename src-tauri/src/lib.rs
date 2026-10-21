mod ai;
mod app_state;
mod audio;
mod batch;
mod capture;
mod clipboard;
mod coca;
mod commands;
mod db;
mod dict;
mod error;
mod export;
mod i18n;
mod logging;
mod online;
mod protocol;
mod query;
mod review;
mod senses;
mod settings;
mod themes;
mod vocab;

use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::ShortcutState;

use app_state::AppState;

/// 持有日志后台线程的 guard，随 App 一起释放。
struct LogGuard(#[allow(dead_code)] tracing_appender::non_blocking::WorkerGuard);

fn close_to_tray(app: &tauri::AppHandle) -> bool {
    app.try_state::<AppState>().and_then(|s| s.settings.get().ok()).is_some_and(|s| s.general.close_to_tray)
}

fn show_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// 托盘图标：单击 / 双击打开主窗口；右键菜单「打开词镜 / 退出」。
fn build_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    let open = MenuItem::with_id(app, "open", i18n::t("打开词镜", "Open Lens"), true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", i18n::t("退出", "Quit"), true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let mut tray = TrayIconBuilder::with_id("lens")
        .tooltip("词镜 Lens")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id().as_ref() {
            "open" => show_main(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();
    // 只允许一个实例：再次打开时把已有的主窗口拿到前台（必须最先注册）。
    // 开发时用 LENS_DATA_DIR 隔开数据的实例不受限制，可与已安装的版本同时运行
    if std::env::var_os("LENS_DATA_DIR").is_none() {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
        }));
    }
    let result = builder
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
        // 关掉主窗口就退出整个程序：隐藏的弹窗 / 导出窗口不能让进程残留，
        // 否则再次打开会被单实例转给这个已没有主窗口的进程，看起来「点了打不开」
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            match event {
                // 设置 › 常规「点击关闭按钮时：隐藏到托盘」：只隐藏主窗口，从托盘菜单退出
                tauri::WindowEvent::CloseRequested { api, .. } if close_to_tray(window.app_handle()) => {
                    api.prevent_close();
                    let _ = window.hide();
                }
                tauri::WindowEvent::Destroyed => window.app_handle().exit(0),
                _ => {}
            }
        })
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
                // 拖动边缘调大小，松开后记住（隐藏时写回设置）
                .resizable(true)
                .min_inner_size(capture::POPUP_MIN_W, capture::POPUP_MIN_H)
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
            if let Err(e) = build_tray(app.handle()) {
                tracing::warn!(error = %e, "创建托盘图标失败");
            }
            app.manage(batch::BatchManager::default());
            // 主窗口配置为先隐藏：取消最大化、缩到屏幕以内并居中之后再显示，
            // 避免一打开就铺满整个屏幕（无边框窗口铺满时看起来像沉浸模式）
            if let Some(main) = app.get_webview_window("main") {
                if main.is_maximized().unwrap_or(false) {
                    let _ = main.unmaximize();
                }
                fit_to_work_area(&main);
                let _ = main.show();
                let _ = main.set_focus();
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::settings::get_settings,
            commands::settings::set_settings,
            commands::settings::app_paths,
            commands::settings::save_custom_theme,
            commands::settings::delete_custom_theme,
            commands::settings::export_theme,
            commands::settings::import_theme,
            commands::dicts::list_dicts,
            commands::dicts::import_dicts,
            commands::dicts::remove_dict,
            commands::dicts::reorder_dicts,
            commands::dicts::list_profiles,
            commands::lookup::lookup,
            commands::lookup::sense_atlas,
            commands::lookup::first_entry,
            commands::ai::coca_status,
            commands::ai::coca_import,
            commands::ai::coca_lookup,
            commands::ai::ai_key_status,
            commands::ai::ai_set_key,
            commands::ai::ai_examples,
            commands::ai::vocab_examples,
            commands::lookup::cancel_lookup,
            commands::lookup::suggest,
            commands::history::record_history,
            commands::history::list_history,
            commands::history::delete_history,
            commands::history::clear_history,
            commands::history::lookup_count,
            commands::history::extend_context,
            commands::words::save_word,
            commands::words::word_id,
            commands::words::list_words,
            commands::words::update_word,
            commands::words::delete_word,
            commands::words::unlink_word_context,
            commands::words::list_tags,
            commands::words::wordbook_stats,
            commands::words::review_counts,
            commands::words::review_queue,
            commands::words::review_rate,
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
            commands::popup::lookup_in_popup,
            commands::popup::save_in_app_context,
        ])
        .run(tauri::generate_context!());

    if let Err(e) = result {
        tracing::error!(error = %e, "Tauri 运行失败");
        eprintln!("Lens 启动失败：{e}");
        std::process::exit(1);
    }
}

/// 默认 1100×760 在小屏（如 1280×720 的 150% 缩放笔记本）上比屏幕还大，又没有系统边框，
/// 看起来像全屏。启动时把主窗口缩到工作区的 80% × 85% 以内，并居中。
fn fit_to_work_area(window: &tauri::WebviewWindow) {
    let (Ok(Some(monitor)), Ok(size)) = (window.current_monitor(), window.outer_size()) else { return };
    let area = monitor.work_area();
    let max_w = (f64::from(area.size.width) * 0.8) as u32;
    let max_h = (f64::from(area.size.height) * 0.85) as u32;
    let w = size.width.min(max_w);
    let h = size.height.min(max_h);
    if w != size.width || h != size.height {
        let _ = window.set_size(tauri::PhysicalSize::new(w, h));
    }
    let x = area.position.x + ((area.size.width - w) / 2) as i32;
    let y = area.position.y + ((area.size.height - h) / 2) as i32;
    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
}
