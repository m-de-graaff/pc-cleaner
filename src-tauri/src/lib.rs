pub mod commands;
pub mod executor;
pub mod fsutil;
pub mod journal;
pub mod model;
pub mod protect;
pub mod scanners;
pub mod scoring;
pub mod sys;

use commands::AppState;
use protect::ProtectedSet;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = app.path().app_local_data_dir()?;
            app.manage(AppState {
                last_scan: Default::default(),
                journal: journal::Journal::new(data_dir.join("journal.jsonl")),
                protect: ProtectedSet::from_system(),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::scan_junk,
            commands::scan_devtools,
            commands::clean,
            commands::history,
            commands::system_info,
            commands::relaunch_as_admin
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
