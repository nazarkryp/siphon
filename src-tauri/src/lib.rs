mod clipboard;
mod commands;
mod core;
mod engine;
mod job;
mod model;
mod net;
mod resolve;
mod store;
#[cfg(test)]
mod test_server;
mod tools;
mod tools_install;
mod util;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let default_folder = app
                .path()
                .download_dir()
                .or_else(|_| app.path().home_dir())
                .unwrap_or_else(|_| data_dir.clone());
            let settings = store::load_settings(&data_dir, default_folder);
            let downloads = store::load_downloads(&data_dir);
            let core = core::Core::new(app.handle().clone(), data_dir, settings, downloads);
            app.manage(core.clone());

            let detect = core.clone();
            tauri::async_runtime::spawn(async move {
                detect.detect_tools().await;
            });
            core.start_ticker();
            clipboard::start_watcher(core);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::resolve_link,
            commands::add_download,
            commands::add_links,
            commands::pause_download,
            commands::resume_download,
            commands::start_now,
            commands::retry_download,
            commands::cancel_download,
            commands::remove_download,
            commands::update_options,
            commands::save_settings,
            commands::detect_tools,
            commands::check_tool_updates,
            commands::install_tool,
            commands::pick_folder,
            commands::free_space,
            commands::open_file,
            commands::show_in_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
