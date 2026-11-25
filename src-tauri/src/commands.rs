//! 前端可调用的命令。配置写入用户目录，不放进任何一个 Git 仓库。

use std::path::Path;

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::State;

use repounion_core::{
    absorb_discovered, add_standalone, commit_folder, create_workspace as create_ws,
    delete_workspace as delete_ws, fetch_all_with as fetch_repos, list_changes,
    load_file_diff as read_file_diff, load_state, normalize_dir, normalize_repo,
    query_statuses_with, remove_repo as remove_from_ws, rename_workspace as rename_ws, save_state,
    scan_folder, scan_git_repos, set_active, set_auto_scan, workspace_mut, AppState, ChangeList,
    FetchProgress, FetchReport, FileDiff, RepoStatus, ScanReport, StatusReport, Workspace,
};

use crate::open_external::{open_folder, open_terminal, open_vscode};
use crate::{ConfigLock, FetchControl};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenResult {
    pub workspace: Workspace,
    pub added: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RescanResult {
    pub workspace: Workspace,
    pub added: usize,
}

fn lock_config(config: &ConfigLock) -> Result<std::sync::MutexGuard<'_, ()>, String> {
    config
        .lock
        .lock()
        .map_err(|_| "内部锁已损坏，请重启应用".to_string())
}

fn with_config<T>(
    config: &ConfigLock,
    update: impl FnOnce(&mut AppState) -> Result<T, String>,
) -> Result<T, String> {
    let _guard = lock_config(config)?;
    let mut state = load_state(&config.path)?;
    let value = update(&mut state)?;
    save_state(&config.path, &state)?;
    Ok(value)
}

fn paths_of(config: &ConfigLock, workspace_id: &str) -> Result<Vec<String>, String> {
    let _guard = lock_config(config)?;
    let state = load_state(&config.path)?;
    let workspace = state
        .workspaces
        .iter()
        .find(|workspace| workspace.id == workspace_id)
        .ok_or("工作区不存在")?;
    Ok(workspace.repo_paths())
}

#[tauri::command]
pub fn get_state(config: State<ConfigLock>) -> Result<AppState, String> {
    let _guard = lock_config(&config)?;
    load_state(&config.path)
}

#[tauri::command]
pub fn create_workspace(config: State<ConfigLock>, name: String) -> Result<Workspace, String> {
    with_config(&config, |state| create_ws(state, &name))
}

#[tauri::command]
pub fn rename_workspace(config: State<ConfigLock>, id: String, name: String) -> Result<(), String> {
    with_config(&config, |state| rename_ws(state, &id, &name))
}

#[tauri::command]
pub fn delete_workspace(config: State<ConfigLock>, id: String) -> Result<AppState, String> {
    with_config(&config, |state| {
        delete_ws(state, &id)?;
        Ok(state.clone())
    })
}

#[tauri::command]
pub async fn open_workspace(
    config: State<'_, ConfigLock>,
    id: String,
) -> Result<OpenResult, String> {
    let sources = {
        let _guard = lock_config(&config)?;
        let state = load_state(&config.path)?;
        let workspace = state
            .workspaces
            .iter()
            .find(|workspace| workspace.id == id)
            .ok_or("工作区不存在")?;
        workspace.sources.clone()
    };
    let to_scan: Vec<(String, String)> = sources
        .into_iter()
        .filter(|source| source.auto_scan)
        .map(|source| (source.id, source.path))
        .collect();

    let discovered = tauri::async_runtime::spawn_blocking(move || {
        to_scan
            .into_iter()
            .map(|(source_id, path)| {
                let found = scan_git_repos(Path::new(&path)).unwrap_or_default();
                let paths = found
                    .into_iter()
                    .map(|repo| repo.to_string_lossy().into_owned())
                    .collect::<Vec<_>>();
                (source_id, paths)
            })
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|err| format!("扫描任务失败：{err}"))?;

    let _guard = lock_config(&config)?;
    let mut state = load_state(&config.path)?;
    set_active(&mut state, Some(id.clone()))?;
    let mut added = 0usize;
    {
        let workspace = workspace_mut(&mut state, &id)?;
        for (source_id, paths) in &discovered {
            if let Ok(count) = absorb_discovered(workspace, source_id, paths) {
                added += count;
            }
        }
    }
    let workspace = state
        .workspaces
        .iter()
        .find(|workspace| workspace.id == id)
        .cloned()
        .ok_or("工作区不存在")?;
    save_state(&config.path, &state)?;
    Ok(OpenResult { workspace, added })
}

#[tauri::command]
pub fn add_standalone_repo(
    config: State<ConfigLock>,
    workspace_id: String,
    path: String,
) -> Result<Workspace, String> {
    let repo = normalize_repo(&path)?;
    let repo = repo.to_string_lossy().into_owned();
    with_config(&config, |state| {
        let workspace = workspace_mut(state, &workspace_id)?;
        add_standalone(workspace, &repo)?;
        Ok(workspace.clone())
    })
}

#[tauri::command]
pub async fn scan_directory(
    config: State<'_, ConfigLock>,
    workspace_id: String,
    path: String,
) -> Result<ScanReport, String> {
    let existing = paths_of(&config, &workspace_id)?;
    let root = normalize_dir(&path)?;
    tauri::async_runtime::spawn_blocking(move || scan_folder(&root, &existing))
        .await
        .map_err(|err| format!("扫描任务失败：{err}"))?
}

#[tauri::command]
pub async fn commit_directory(
    config: State<'_, ConfigLock>,
    workspace_id: String,
    path: String,
    selected: Vec<String>,
    remember_as_source: bool,
) -> Result<Workspace, String> {
    let root = normalize_dir(&path)?;
    let discovered = tauri::async_runtime::spawn_blocking({
        let root = root.clone();
        move || scan_git_repos(&root)
    })
    .await
    .map_err(|err| format!("扫描任务失败：{err}"))??;
    let discovered: Vec<String> = discovered
        .into_iter()
        .map(|repo| repo.to_string_lossy().into_owned())
        .collect();
    let root = root.to_string_lossy().into_owned();
    with_config(&config, |state| {
        let workspace = workspace_mut(state, &workspace_id)?;
        commit_folder(workspace, &root, &discovered, &selected, remember_as_source)?;
        Ok(workspace.clone())
    })
}

#[tauri::command]
pub fn set_source_auto_scan(
    config: State<ConfigLock>,
    workspace_id: String,
    source_id: String,
    auto_scan: bool,
) -> Result<Workspace, String> {
    with_config(&config, |state| {
        let workspace = workspace_mut(state, &workspace_id)?;
        set_auto_scan(workspace, &source_id, auto_scan)?;
        Ok(workspace.clone())
    })
}

#[tauri::command]
pub async fn rescan_source(
    config: State<'_, ConfigLock>,
    workspace_id: String,
    source_id: String,
) -> Result<RescanResult, String> {
    let root = {
        let _guard = lock_config(&config)?;
        let state = load_state(&config.path)?;
        let workspace = state
            .workspaces
            .iter()
            .find(|workspace| workspace.id == workspace_id)
            .ok_or("工作区不存在")?;
        let source = workspace
            .sources
            .iter()
            .find(|source| source.id == source_id)
            .ok_or("来源目录不存在")?;
        source.path.clone()
    };
    let discovered = tauri::async_runtime::spawn_blocking(move || scan_git_repos(Path::new(&root)))
        .await
        .map_err(|err| format!("扫描任务失败：{err}"))??;
    let discovered: Vec<String> = discovered
        .into_iter()
        .map(|repo| repo.to_string_lossy().into_owned())
        .collect();
    with_config(&config, |state| {
        let workspace = workspace_mut(state, &workspace_id)?;
        let added = absorb_discovered(workspace, &source_id, &discovered)?;
        Ok(RescanResult {
            workspace: workspace.clone(),
            added,
        })
    })
}

#[tauri::command]
pub fn remove_repo(
    config: State<ConfigLock>,
    workspace_id: String,
    path: String,
) -> Result<Workspace, String> {
    with_config(&config, |state| {
        let workspace = workspace_mut(state, &workspace_id)?;
        remove_from_ws(workspace, &path)?;
        Ok(workspace.clone())
    })
}

/// 只读本地状态，不 fetch。每完成一个仓库就通过 `on_item` 推送一行，最后返回完整结果。
#[tauri::command]
pub async fn refresh_status(
    config: State<'_, ConfigLock>,
    workspace_id: String,
    on_item: Channel<RepoStatus>,
) -> Result<StatusReport, String> {
    let paths = paths_of(&config, &workspace_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        query_statuses_with(&paths, |item| {
            // 前端窗口已关闭时发送会失败，忽略即可，最终结果仍会返回。
            let _ = on_item.send(item.clone());
        })
    })
    .await
    .map_err(|err| format!("状态查询任务失败：{err}"))
}

/// 对每个仓库 `git fetch`，然后重新读取状态。不合并本地分支。
/// 同一时间只允许一批 Fetch；`cancel_fetch` 可以取消当前批次。
#[tauri::command]
pub async fn fetch_all(
    config: State<'_, ConfigLock>,
    control: State<'_, FetchControl>,
    workspace_id: String,
    on_item: Channel<FetchProgress>,
) -> Result<FetchReport, String> {
    let paths = paths_of(&config, &workspace_id)?;
    let cancel = control.begin()?;
    let worker_cancel = cancel.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        fetch_repos(&paths, &worker_cancel, |progress| {
            let _ = on_item.send(progress.clone());
        })
    })
    .await
    .map_err(|err| format!("Fetch 任务失败：{err}"));
    control.finish();
    result
}

/// 取消正在进行的 Fetch。没有进行中的 Fetch 时什么也不做。
#[tauri::command]
pub fn cancel_fetch(control: State<FetchControl>) {
    control.cancel();
}

/// 只读取这一个仓库的改动文件，不预取 diff。
#[tauri::command]
pub async fn list_repo_changes(path: String, mode: String) -> Result<ChangeList, String> {
    match tauri::async_runtime::spawn_blocking(move || list_changes(&path, &mode)).await {
        Ok(result) => result,
        Err(err) => Err(format!("读取改动失败：{err}")),
    }
}

/// 只读取当前这一个文件的左右内容和差异。
#[tauri::command]
pub async fn load_file_diff(
    path: String,
    mode: String,
    file_path: String,
    old_path: Option<String>,
) -> Result<FileDiff, String> {
    match tauri::async_runtime::spawn_blocking(move || {
        read_file_diff(&path, &mode, &file_path, old_path)
    })
    .await
    {
        Ok(result) => result,
        Err(err) => Err(format!("读取差异失败：{err}")),
    }
}

#[tauri::command]
pub fn open_in_folder(path: String) -> Result<(), String> {
    open_folder(Path::new(&path))
}

#[tauri::command]
pub fn open_in_vscode(path: String) -> Result<(), String> {
    open_vscode(Path::new(&path))
}

#[tauri::command]
pub fn open_in_terminal(path: String) -> Result<(), String> {
    open_terminal(Path::new(&path))
}
