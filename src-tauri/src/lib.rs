pub mod bt;
pub mod catalog;
pub mod commands;
pub mod dates;
pub mod detail;
pub mod ipc;
pub mod model;
pub mod net;
pub mod net_reqwest;
pub mod platform;
pub mod search;
pub mod state;
pub mod storage;
pub mod swarm;
pub mod text;
pub mod update;

use std::sync::Arc;

use tauri::{Emitter as _, Manager};

use crate::net::Downloader;
use crate::net_reqwest::ReqwestClient;
use crate::platform::SystemPlatform;
use crate::state::{AppState, Emitter};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle().clone();
            let emitter: Emitter = Arc::new(move |event, payload| {
                let _ = handle.emit(event, payload);
            });
            let client = ReqwestClient::new()?;
            let downloader = Arc::new(Downloader::new(Arc::new(client)));
            let state = AppState::new(
                storage::sources_dir(),
                downloader,
                Arc::new(SystemPlatform::new()),
                emitter,
            );
            app.manage(Arc::clone(&state));
            if let Some(window) = app.get_webview_window("main") {
                if let Ok(Some(monitor)) = window.current_monitor() {
                    let scale = monitor.scale_factor();
                    let logical = monitor.size().to_logical::<f64>(scale);
                    let width = (logical.width - 24.0).clamp(900.0, 1440.0);
                    let height = (logical.height - 84.0).clamp(620.0, 900.0);
                    let _ = window.set_size(tauri::LogicalSize::new(width, height));
                    let _ = window.set_min_size(Some(tauri::LogicalSize::new(900.0, 620.0)));
                    let _ = window.center();
                }
            }
            state.start();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ipc::get_snapshot,
            ipc::get_results,
            ipc::get_detail,
            ipc::get_swarm_texts,
            ipc::load_sources,
            ipc::set_query,
            ipc::set_fuzzy,
            ipc::set_threshold,
            ipc::select_source,
            ipc::toggle_sort,
            ipc::select_result,
            ipc::copy_current_links,
            ipc::request_detail_raw,
            ipc::set_window_theme,
            ipc::copy_text,
            ipc::open_sources_folder,
            ipc::open_external_url,
            ipc::add_sources,
            ipc::request_update_source,
            ipc::delete_active_source,
            ipc::import_sources,
            ipc::pick_import_files,
            ipc::refresh_all,
            ipc::check_updates,
            ipc::query_swarm,
            ipc::send_to_bt,
            ipc::refresh_bt_targets,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
