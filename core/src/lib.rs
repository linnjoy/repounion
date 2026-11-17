//! RepoUnion 核心：工作区配置、目录扫描和 Git 状态。
//!
//! 图形界面放在 Tauri 壳里。这个 crate 不依赖系统图形库，单元测试可以直接跑。

#![forbid(unsafe_code)]

mod changes;
mod gitinfo;
mod model;
mod parallel;
mod paths;
mod scan;
mod store;

pub use changes::{list_changes, load_file_diff, ChangeList, FileChange, FileDiff, HunkSpan};
pub use gitinfo::{
    fetch_all, query_statuses, FetchFailure, FetchReport, RepoStatus, StatusReport, Tone,
    GIT_CONCURRENCY,
};
pub use model::{AppState, SourceDirectory, Workspace};
pub use paths::{
    default_config_path, is_git_checkout, normalize_dir, normalize_repo, path_key, repo_name,
};
pub use scan::{scan_folder, scan_git_repos, ScanEntry, ScanReport};
pub use store::{
    absorb_discovered, add_standalone, commit_folder, create_workspace, delete_workspace,
    load_state, remove_repo, rename_workspace, save_state, set_active, set_auto_scan,
    workspace_mut,
};
