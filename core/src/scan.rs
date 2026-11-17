//! 递归查找 Git 工作树。
//!
//! 只在用户指定的目录里找 `.git`，不监视文件变化。
//! `node_modules`、`.git` 内部以及常见构建产物目录里偶然出现的嵌套仓库会被跳过。

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::gitinfo::{badge_label, query_statuses, Tone};
use crate::paths::{is_git_checkout, path_key, repo_name};

/// 从根目录往下最多再走这么多层，避免误选家目录时扫得过深。
pub const MAX_SCAN_DEPTH: usize = 10;

const SKIP_DIR_NAMES: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    "out",
    "bin",
    "obj",
    ".next",
    ".nuxt",
    ".output",
    "coverage",
    ".turbo",
    ".cache",
    ".venv",
    "venv",
    "__pycache__",
    "vendor",
    "pods",
    ".gradle",
    "deriveddata",
];

pub fn should_skip_dir(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    SKIP_DIR_NAMES.iter().any(|skip| *skip == lower)
}

pub fn scan_git_repos(root: &Path) -> Result<Vec<PathBuf>, String> {
    scan_git_repos_limited(root, MAX_SCAN_DEPTH)
}

pub fn scan_git_repos_limited(root: &Path, max_depth: usize) -> Result<Vec<PathBuf>, String> {
    if !root.exists() {
        return Err("路径不存在".into());
    }
    if !root.is_dir() {
        return Err("请选择文件夹".into());
    }
    let mut found = Vec::new();
    walk(root, 0, max_depth, &mut found);
    found.sort();
    found.dedup();
    Ok(found)
}

fn walk(dir: &Path, depth: usize, max_depth: usize, found: &mut Vec<PathBuf>) {
    if is_git_checkout(dir) {
        found.push(dir.to_path_buf());
    }
    if depth >= max_depth {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(_) => continue,
        };
        // 不跟随符号链接，避免循环，也避免扫到目录外面。
        if file_type.is_symlink() || !file_type.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if should_skip_dir(&name) {
            continue;
        }
        walk(&entry.path(), depth + 1, max_depth, found);
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScanEntry {
    pub path: String,
    pub name: String,
    pub branch: String,
    pub tone: Tone,
    pub badge: String,
    pub already_in_workspace: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    pub root: String,
    pub entries: Vec<ScanEntry>,
}

/// 扫描目录并读取每个仓库的分支和粗略状态，供添加前勾选。
pub fn scan_folder(root: &Path, existing: &[String]) -> Result<ScanReport, String> {
    let root = dunce::canonicalize(root).map_err(|err| format!("无法解析路径：{err}"))?;
    let found = scan_git_repos(&root)?;
    let existing_keys: std::collections::HashSet<String> = existing
        .iter()
        .map(|path| path_key(Path::new(path)))
        .collect();
    let paths: Vec<String> = found
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    let report = query_statuses(&paths);
    let mut entries = Vec::with_capacity(report.items.len());
    for item in report.items {
        let already = existing_keys.contains(&path_key(Path::new(&item.path)));
        let badge = if already {
            "已在工作区".to_string()
        } else {
            badge_label(item.tone).to_string()
        };
        entries.push(ScanEntry {
            name: repo_name(&item.path),
            path: item.path,
            branch: item.branch,
            tone: item.tone,
            badge,
            already_in_workspace: already,
        });
    }
    entries.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.path.cmp(&right.path))
    });
    Ok(ScanReport {
        root: root.to_string_lossy().into_owned(),
        entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn touch_repo(dir: &Path) {
        fs::create_dir_all(dir.join(".git")).unwrap();
    }

    fn touch_gitdir_file(dir: &Path) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join(".git"), "gitdir: ../.git/modules/linked\n").unwrap();
    }

    #[test]
    fn finds_directory_and_gitdir_file() {
        let root = tempfile::tempdir().unwrap();
        touch_repo(&root.path().join("app"));
        touch_gitdir_file(&root.path().join("linked"));
        fs::create_dir_all(root.path().join("notes")).unwrap();
        fs::write(root.path().join("notes").join("readme.txt"), "hi").unwrap();

        let found = scan_git_repos(root.path()).unwrap();
        let names = found
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["app".to_string(), "linked".to_string()]);
    }

    #[test]
    fn skips_dependencies_and_build_output() {
        let root = tempfile::tempdir().unwrap();
        touch_repo(&root.path().join("app"));
        touch_repo(&root.path().join("services").join("api"));
        touch_repo(
            &root
                .path()
                .join("app")
                .join("node_modules")
                .join("left-pad"),
        );
        touch_repo(&root.path().join("Node_Modules").join("pkg"));
        touch_repo(&root.path().join("app").join("target").join("debug-helper"));
        touch_repo(&root.path().join("web").join("dist").join("vendor"));
        touch_repo(&root.path().join("web").join("build").join("tmp"));
        touch_repo(&root.path().join("native").join("obj").join("nested"));
        // `.git` 里面的 modules 不能被当成另一个仓库。
        let modules = root
            .path()
            .join("app")
            .join(".git")
            .join("modules")
            .join("sub");
        touch_repo(&modules);

        let found = scan_git_repos(root.path()).unwrap();
        let rel = found
            .iter()
            .map(|path| {
                path.strip_prefix(root.path())
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect::<Vec<_>>();
        assert_eq!(rel, vec!["app".to_string(), "services/api".to_string()]);
    }

    #[test]
    fn ignores_plain_file_named_git() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("not-repo");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(".git"), "this is not a gitdir\n").unwrap();
        let found = scan_git_repos(root.path()).unwrap();
        assert!(found.is_empty());
    }

    #[test]
    fn depth_limit_stops_descent() {
        let root = tempfile::tempdir().unwrap();
        touch_repo(&root.path().join("a"));
        touch_repo(&root.path().join("a").join("b").join("c"));
        let found = scan_git_repos_limited(root.path(), 1).unwrap();
        assert_eq!(found.len(), 1);
        assert!(found[0].ends_with("a"));
    }

    #[test]
    fn does_not_follow_symlink() {
        let root = tempfile::tempdir().unwrap();
        let real = root.path().join("real");
        touch_repo(&real);
        let link = root.path().join("link");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&real, &link).unwrap();
            let found = scan_git_repos(root.path()).unwrap();
            assert_eq!(found.len(), 1);
            assert!(found[0].ends_with("real"));
        }
    }
}
