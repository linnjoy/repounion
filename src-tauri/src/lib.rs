//! Tauri 壳：把核心库的能力暴露给界面。

mod commands;
mod open_external;

use std::path::PathBuf;
use std::sync::Mutex;

use repounion_core::CancelToken;
use tauri::Manager;

pub struct ConfigLock {
    pub path: PathBuf,
    pub lock: Mutex<()>,
}

/// 记录当前这一批 Fetch 的取消标志。同一时间只允许一批 Fetch。
#[derive(Default)]
pub struct FetchControl {
    current: Mutex<Option<CancelToken>>,
}

impl FetchControl {
    /// 开始新一批 Fetch。已有进行中的批次时拒绝，避免两批互相抢 Git 名额。
    pub fn begin(&self) -> Result<CancelToken, String> {
        let mut current = self
            .current
            .lock()
            .map_err(|_| "内部锁已损坏，请重启应用".to_string())?;
        if current.is_some() {
            return Err("已有 Fetch 正在进行".to_string());
        }
        let token = CancelToken::new();
        *current = Some(token.clone());
        Ok(token)
    }

    pub fn finish(&self) {
        if let Ok(mut current) = self.current.lock() {
            *current = None;
        }
    }

    pub fn cancel(&self) {
        if let Ok(current) = self.current.lock() {
            if let Some(token) = current.as_ref() {
                token.cancel();
            }
        }
    }
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
            app.manage(FetchControl::default());
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
            commands::cancel_fetch,
            commands::list_repo_changes,
            commands::load_file_diff,
            commands::open_in_folder,
            commands::open_in_vscode,
            commands::open_in_terminal,
        ])
        .run(tauri::generate_context!())
        .expect("启动 RepoUnion 失败");
}
