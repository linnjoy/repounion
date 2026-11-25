//! 「有新变化」的快照。
//!
//! 每个仓库记住用户上次「标记已查看」时的状态指纹。之后刷新得到的指纹不同，就标记为有新变化，
//! 直到用户再次确认。快照单独保存在 `snapshots.json`，不和工作区配置混在一起，
//! 避免每次刷新都改写配置文件。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::gitinfo::RepoStatus;
use crate::paths::path_key;
use crate::store::write_text_atomic;

const SNAPSHOT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotStore {
    pub version: u32,
    /// 键为规范化后的仓库路径（`path_key`）。
    #[serde(default)]
    pub repos: BTreeMap<String, AckEntry>,
}

impl Default for SnapshotStore {
    fn default() -> Self {
        Self {
            version: SNAPSHOT_VERSION,
            repos: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AckEntry {
    pub fingerprint: String,
    /// 确认时间（Unix 秒）。首次见到仓库时自动建立基线，也记一次。
    pub acked_at: i64,
}

/// 前端确认某个仓库时，带上它当时看到的指纹。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AckRequest {
    pub path: String,
    pub fingerprint: String,
}

pub fn default_snapshot_path() -> Result<PathBuf, String> {
    let base = dirs::config_dir().ok_or("无法定位用户配置目录")?;
    Ok(base.join("RepoUnion").join("snapshots.json"))
}

/// 读取快照。文件不存在或已损坏时返回空快照：最坏情况只是所有仓库重新建立基线，不影响使用。
pub fn load_snapshots(path: &Path) -> SnapshotStore {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_snapshots(path: &Path, store: &SnapshotStore) -> Result<(), String> {
    let json =
        serde_json::to_string_pretty(store).map_err(|err| format!("无法序列化快照：{err}"))?;
    write_text_atomic(path, &json)
}

impl SnapshotStore {
    /// 根据快照填写 `changed_since_ack`。返回快照是否被修改（需要保存）。
    ///
    /// 第一次见到的仓库以当前状态为基线，不标记为有新变化；
    /// 否则首次启动或新加入仓库时，所有仓库都会被标成「有新变化」，失去提示意义。
    pub fn observe(&mut self, status: &mut RepoStatus, now: i64) -> bool {
        // 读取失败的仓库没有可靠指纹，不比较也不改基线。
        if !status.ok || status.fingerprint.is_empty() {
            status.changed_since_ack = false;
            return false;
        }
        let key = path_key(Path::new(&status.path));
        match self.repos.get(&key) {
            Some(entry) => {
                status.changed_since_ack = entry.fingerprint != status.fingerprint;
                false
            }
            None => {
                self.repos.insert(
                    key,
                    AckEntry {
                        fingerprint: status.fingerprint.clone(),
                        acked_at: now,
                    },
                );
                status.changed_since_ack = false;
                true
            }
        }
    }

    /// 把仓库的基线更新为用户看到的指纹。
    /// 用前端看到的指纹而不是重新读取：用户确认之后才发生的变化仍会被提示。
    pub fn ack(&mut self, requests: &[AckRequest], now: i64) {
        for request in requests {
            if request.fingerprint.is_empty() {
                continue;
            }
            self.repos.insert(
                path_key(Path::new(&request.path)),
                AckEntry {
                    fingerprint: request.fingerprint.clone(),
                    acked_at: now,
                },
            );
        }
    }
}

pub fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gitinfo::{status_of, tests_support::git};
    use std::fs;

    fn init_repo(dir: &Path) {
        git(dir, &["init", "-b", "main"]);
        git(dir, &["config", "user.email", "t@example.com"]);
        git(dir, &["config", "user.name", "tester"]);
        fs::write(dir.join("a.txt"), "v1\n").unwrap();
        git(dir, &["add", "a.txt"]);
        git(dir, &["commit", "-m", "init"]);
    }

    #[test]
    fn first_sight_is_baseline_then_changes_are_flagged_until_ack() {
        if !crate::gitinfo::tests_support::git_ok() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        let path = dir.path().to_string_lossy().into_owned();
        let mut store = SnapshotStore::default();

        let mut first = status_of(&path, &[]);
        assert!(store.observe(&mut first, 1), "首次见到应建立基线");
        assert!(!first.changed_since_ack);

        let mut same = status_of(&path, &[]);
        assert!(!store.observe(&mut same, 2));
        assert!(!same.changed_since_ack, "状态没变不应提示");

        fs::write(dir.path().join("a.txt"), "v2\n").unwrap();
        let mut edited = status_of(&path, &[]);
        store.observe(&mut edited, 3);
        assert!(edited.changed_since_ack, "新增改动应提示");

        store.ack(
            &[AckRequest {
                path: path.clone(),
                fingerprint: edited.fingerprint.clone(),
            }],
            4,
        );
        let mut after_ack = status_of(&path, &[]);
        store.observe(&mut after_ack, 5);
        assert!(!after_ack.changed_since_ack, "确认后不再提示");

        // 已经修改过的文件再次被修改，Git 状态码不变，但仍应被发现。
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(dir.path().join("a.txt"), "v3 更长的内容\n").unwrap();
        let mut edited_again = status_of(&path, &[]);
        store.observe(&mut edited_again, 6);
        assert!(edited_again.changed_since_ack, "已改文件再次修改也应提示");
    }

    #[test]
    fn new_commit_on_other_branch_is_flagged() {
        if !crate::gitinfo::tests_support::git_ok() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        let path = dir.path().to_string_lossy().into_owned();
        let mut store = SnapshotStore::default();
        let mut base = status_of(&path, &[]);
        store.observe(&mut base, 1);

        // 不切换当前分支，直接在另一个分支上产生提交（类似 agent 在 worktree 里提交）。
        git(dir.path(), &["branch", "agent"]);
        let mut with_branch = status_of(&path, &[]);
        store.observe(&mut with_branch, 2);
        assert!(with_branch.changed_since_ack, "新建分支应提示");
    }

    #[test]
    fn corrupt_file_loads_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("snapshots.json");
        fs::write(&path, "not json").unwrap();
        assert_eq!(load_snapshots(&path), SnapshotStore::default());
        let mut store = SnapshotStore::default();
        store.ack(
            &[AckRequest {
                path: "D:/work/app".into(),
                fingerprint: "abc".into(),
            }],
            7,
        );
        save_snapshots(&path, &store).unwrap();
        assert_eq!(load_snapshots(&path), store);
    }
}
