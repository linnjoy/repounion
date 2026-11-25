//! RepoUnion 核心：工作区配置、目录扫描和 Git 状态。
//!
//! 图形界面放在 Tauri 壳里。这个 crate 不依赖系统图形库，单元测试可以直接跑。

#![forbid(unsafe_code)]

mod changes;
mod gitinfo;
mod insight;
mod model;
mod parallel;
mod paths;
mod scan;
mod snapshot;
mod store;

pub use changes::{list_changes, load_file_diff, ChangeList, FileChange, FileDiff, HunkSpan};
pub use gitinfo::{
    fetch_all, fetch_all_with, query_statuses, query_statuses_with, CancelToken, FetchFailure,
    FetchProgress, FetchReport, RepoStatus, StatusReport, Tone, CANCELLED_MESSAGE, GIT_CONCURRENCY,
    LOCAL_TIMEOUT, NETWORK_TIMEOUT,
};
pub use insight::{branch_matches, is_protected, BranchInfo, WorktreeInfo};
pub use model::{default_protected_branches, AppState, SourceDirectory, Workspace, CONFIG_VERSION};
pub use paths::{
    default_config_path, is_git_checkout, normalize_dir, normalize_repo, path_key, repo_name,
};
pub use scan::{scan_folder, scan_git_repos, ScanEntry, ScanReport};
pub use snapshot::{
    default_snapshot_path, load_snapshots, save_snapshots, unix_now, AckEntry, AckRequest,
    SnapshotStore,
};
pub use store::{
    absorb_discovered, add_standalone, commit_folder, create_workspace, delete_workspace,
    load_state, remove_repo, rename_workspace, save_state, set_active, set_auto_scan,
    set_protected_branches, workspace_mut,
};
