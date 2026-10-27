//! Tauri commands exposed to the web UI.

use std::sync::Arc;

use serde_json::Value;
use tauri::{Manager, State};

use crate::state::AppState;

pub type SharedState = Arc<AppState>;

fn shared(state: &State<'_, SharedState>) -> Arc<AppState> {
    Arc::clone(state.inner())
}

#[tauri::command]
pub fn get_snapshot(state: State<'_, SharedState>) -> Value {
    serde_json::to_value(shared(&state).snapshot()).unwrap_or(Value::Null)
}

#[tauri::command]
pub fn get_results(state: State<'_, SharedState>) -> Value {
    serde_json::to_value(shared(&state).results_rows()).unwrap_or(Value::Null)
}

#[tauri::command]
pub fn get_detail(state: State<'_, SharedState>) -> Value {
    serde_json::to_value(shared(&state).detail_payload()).unwrap_or(Value::Null)
}

#[tauri::command]
pub fn get_swarm_texts(state: State<'_, SharedState>) -> Value {
    serde_json::to_value(shared(&state).swarm_texts()).unwrap_or(Value::Null)
}

#[tauri::command]
pub fn load_sources(state: State<'_, SharedState>) {
    shared(&state).load_sources();
}

#[tauri::command]
pub fn set_query(state: State<'_, SharedState>, text: String) {
    shared(&state).set_query(&text);
}

#[tauri::command]
pub fn set_fuzzy(state: State<'_, SharedState>, enabled: bool) {
    shared(&state).set_fuzzy(enabled);
}

#[tauri::command]
pub fn set_threshold(state: State<'_, SharedState>, value: f64) {
    shared(&state).set_threshold(value);
}

#[tauri::command]
pub fn select_source(state: State<'_, SharedState>, name: String) {
    shared(&state).select_source(&name);
}

#[tauri::command]
pub fn toggle_sort(state: State<'_, SharedState>) {
    shared(&state).toggle_sort();
}

#[tauri::command]
pub fn set_view(state: State<'_, SharedState>, view: String) {
    shared(&state).set_view(&view);
}

#[tauri::command]
pub fn set_show_handled(state: State<'_, SharedState>, enabled: bool) {
    shared(&state).set_show_handled(enabled);
}

#[tauri::command]
pub fn toggle_favorite(state: State<'_, SharedState>, key: String) {
    shared(&state).toggle_favorite(&key);
}

#[tauri::command]
pub fn toggle_handled(state: State<'_, SharedState>, key: String) {
    shared(&state).toggle_handled(&key);
}

#[tauri::command]
pub fn set_blocklist(state: State<'_, SharedState>, terms: Vec<String>) {
    shared(&state).set_blocklist(terms);
}

#[tauri::command]
pub fn set_block_enabled(state: State<'_, SharedState>, enabled: bool) {
    shared(&state).set_block_enabled(enabled);
}

#[tauri::command]
pub fn select_result(state: State<'_, SharedState>, index: i64) {
    shared(&state).select_result(index);
}

#[tauri::command]
pub fn request_detail_raw(state: State<'_, SharedState>) {
    shared(&state).request_detail_raw();
}

#[tauri::command]
pub fn set_window_theme(app: tauri::AppHandle, light: bool) {
    if let Some(window) = app.get_webview_window("main") {
        let theme = if light {
            tauri::Theme::Light
        } else {
            tauri::Theme::Dark
        };
        let _ = window.set_theme(Some(theme));
        #[cfg(windows)]
        if let Ok(handle) = window.hwnd() {
            use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
            // COLORREF is 0x00BBGGRR: dark topbar #0c1118, light topbar #f6f8fc.
            let (dark, caption, text, border) = if light {
                (0i32, 0x00fc_f8f6u32, 0x0026_1a13u32, 0x00f0_e7e2u32)
            } else {
                (1i32, 0x0018_110cu32, 0x00f7_eee9u32, 0x0037_2b22u32)
            };
            unsafe {
                DwmSetWindowAttribute(
                    handle.0 as *mut std::ffi::c_void,
                    20,
                    &dark as *const i32 as *const std::ffi::c_void,
                    std::mem::size_of::<i32>() as u32,
                );
                for (attribute, value) in [(35u32, caption), (36u32, text), (34u32, border)] {
                    DwmSetWindowAttribute(
                        handle.0 as *mut std::ffi::c_void,
                        attribute,
                        &value as *const u32 as *const std::ffi::c_void,
                        std::mem::size_of::<u32>() as u32,
                    );
                }
            }
        }
    }
}

#[tauri::command]
pub fn copy_current_links(state: State<'_, SharedState>) {
    shared(&state).copy_current_links();
}

#[tauri::command]
pub fn copy_text(state: State<'_, SharedState>, text: String) {
    shared(&state).copy_text(&text);
}

#[tauri::command]
pub fn open_sources_folder(state: State<'_, SharedState>) {
    shared(&state).open_sources_folder();
}

#[tauri::command]
pub fn open_external_url(state: State<'_, SharedState>, url: String) {
    shared(&state).open_external_url(&url);
}

#[tauri::command]
pub fn add_sources(state: State<'_, SharedState>, urls: Vec<String>, overwrite: bool) {
    shared(&state).add_sources(urls, overwrite);
}

#[tauri::command]
pub fn request_update_source(state: State<'_, SharedState>, url: String) {
    shared(&state).request_update_source(&url);
}

#[tauri::command]
pub fn delete_active_source(state: State<'_, SharedState>) {
    shared(&state).delete_active_source();
}

#[tauri::command]
pub fn import_sources(state: State<'_, SharedState>, paths: Vec<String>) {
    shared(&state).import_sources(paths);
}

#[tauri::command]
pub fn pick_import_files(state: State<'_, SharedState>) {
    let state = shared(&state);
    std::thread::spawn(move || {
        let Some(files) = rfd::FileDialog::new()
            .add_filter("JSON", &["json", "gz"])
            .pick_files()
        else {
            return;
        };
        let paths: Vec<String> = files
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect();
        state.import_sources(paths);
    });
}

#[tauri::command]
pub fn refresh_all(state: State<'_, SharedState>) {
    shared(&state).refresh_all();
}

#[tauri::command]
pub fn check_updates(state: State<'_, SharedState>) {
    shared(&state).check_updates();
}

#[tauri::command]
pub fn install_update(state: State<'_, SharedState>) {
    shared(&state).install_update();
}

#[tauri::command]
pub fn query_swarm(state: State<'_, SharedState>, uri: String) {
    shared(&state).query_swarm(&uri);
}

#[tauri::command]
pub fn send_to_bt(state: State<'_, SharedState>, uri: String, key: String) {
    shared(&state).send_to_bt(&uri, &key);
}

#[tauri::command]
pub fn refresh_bt_targets(state: State<'_, SharedState>) {
    shared(&state).refresh_bt_targets();
}
