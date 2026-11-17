//! Tauri 壳：把核心库的能力暴露给界面。

mod commands;
mod open_external;

use std::path::PathBuf;
use std::sync::Mutex;

use tauri::Manager;

pub struct ConfigLock {
    pub path: PathBuf,
    pub lock: Mutex<()>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let path = repounion_core::default_config_path().map_err(std::io::Error::other)?;
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            app.manage(ConfigLock {
                path,
                lock: Mutex::new(()),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::create_workspace,
            commands::rename_workspace,
            commands::delete_workspace,
            commands::open_workspace,
            commands::add_standalone_repo,
            commands::scan_directory,
            commands::commit_directory,
            commands::set_source_auto_scan,
            commands::rescan_source,
            commands::remove_repo,
            commands::refresh_status,
            commands::fetch_all,
            commands::open_in_folder,
            commands::open_in_vscode,
            commands::open_in_terminal,
        ])
        .run(tauri::generate_context!())
        .expect("启动 RepoUnion 失败");
}
