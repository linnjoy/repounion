//! 「看清」所需的附加信息：所有本地分支的推送情况、linked worktree、保护分支匹配、改动指纹。
//!
//! 这些信息全部来自本地 Git 数据，不访问网络。任何一项读取失败都不影响主状态，
//! 只在 `RepoStatus::notes` 里记一条说明。

use std::path::Path;
use std::process::Output;

use serde::Serialize;

use crate::changes::{take_fields, unquote_git_path};
use crate::gitinfo::{git_command, git_output, stderr_brief};

/// 没有上游的分支需要单独跑 `rev-list` 统计未推送提交。超过这个数量的分支不再统计，
/// 避免分支特别多的仓库拖慢整体刷新。
const MAX_UNTRACKED_BRANCH_COUNTS: usize = 30;

/// 本地分支的推送情况。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BranchInfo {
    pub name: String,
    /// 分支指向的提交号。
    pub oid: String,
    pub is_current: bool,
    pub upstream: Option<String>,
    /// 配置了上游，但上游分支已在远端删除（常见于合并后删分支）。
    pub upstream_gone: bool,
    pub ahead: u32,
    pub behind: u32,
    /// 不在任何远端引用上的提交数。有上游时等于 ahead；
    /// 无上游时用 `rev-list --not --remotes` 统计；未统计时为 None。
    pub unpushed: Option<u32>,
    pub last_commit_at: Option<i64>,
}

/// linked worktree（不含主工作区）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeInfo {
    pub path: String,
    pub branch: Option<String>,
    pub head: String,
    /// 有改动（含未跟踪）的文件数；读取失败时为 None。
    pub changed_files: Option<u32>,
    /// worktree 目录已不存在，Git 标记为可清理。
    pub prunable: bool,
}

/// 读取所有本地分支及推送情况。返回分支列表，以及仓库是否有任何远端引用。
pub(crate) fn read_branches(repo: &Path) -> Result<(Vec<BranchInfo>, bool), String> {
    // %00 分隔字段，避免分支名里的空格等字符影响解析。
    let output = run_in(
        repo,
        &[
            "for-each-ref",
            "--format=%(refname)%00%(HEAD)%00%(upstream:short)%00%(upstream:track,nobracket)%00%(committerdate:unix)%00%(objectname)",
            "refs/heads",
            "refs/remotes",
        ],
    )?;
    let (mut branches, has_remote) = parse_for_each_ref(&String::from_utf8_lossy(&output.stdout));

    // 只有存在远端引用时，「不在任何远端上的提交」才有意义；纯本地仓库一律不统计。
    if has_remote {
        let mut counted = 0usize;
        for branch in branches.iter_mut() {
            if branch.unpushed.is_some() {
                continue;
            }
            if counted >= MAX_UNTRACKED_BRANCH_COUNTS {
                break;
            }
            counted += 1;
            let refname = format!("refs/heads/{}", branch.name);
            if let Ok(output) = run_in(
                repo,
                &["rev-list", "--count", &refname, "--not", "--remotes"],
            ) {
                branch.unpushed = String::from_utf8_lossy(&output.stdout).trim().parse().ok();
            }
        }
    }
    Ok((branches, has_remote))
}

pub(crate) fn parse_for_each_ref(text: &str) -> (Vec<BranchInfo>, bool) {
    let mut branches = Vec::new();
    let mut has_remote = false;
    for line in text.lines() {
        let fields: Vec<&str> = line.trim_end_matches('\r').split('\0').collect();
        if fields.len() < 6 {
            continue;
        }
        let refname = fields[0];
        if refname.starts_with("refs/remotes/") {
            has_remote = true;
            continue;
        }
        let Some(name) = refname.strip_prefix("refs/heads/") else {
            continue;
        };
        let upstream = Some(fields[2].to_string()).filter(|text| !text.is_empty());
        let (ahead, behind, gone) = parse_track(fields[3]);
        // 有效上游时 ahead 就是未推送数；上游已删除或无上游时需要另行统计。
        let unpushed = if upstream.is_some() && !gone {
            Some(ahead)
        } else {
            None
        };
        branches.push(BranchInfo {
            name: name.to_string(),
            oid: fields[5].trim().to_string(),
            is_current: fields[1] == "*",
            upstream,
            upstream_gone: gone,
            ahead,
            behind,
            unpushed,
            last_commit_at: fields[4].trim().parse().ok(),
        });
    }
    (branches, has_remote)
}

/// 解析 `%(upstream:track,nobracket)`：`ahead 1, behind 2`、`ahead 3`、`gone` 或空。
fn parse_track(text: &str) -> (u32, u32, bool) {
    let text = text.trim();
    if text == "gone" {
        return (0, 0, true);
    }
    let mut ahead = 0;
    let mut behind = 0;
    for part in text.split(',') {
        let mut words = part.split_whitespace();
        match (
            words.next(),
            words.next().and_then(|count| count.parse().ok()),
        ) {
            (Some("ahead"), Some(count)) => ahead = count,
            (Some("behind"), Some(count)) => behind = count,
            _ => {}
        }
    }
    (ahead, behind, false)
}

/// 读取 linked worktree 及各自的改动数。没有 linked worktree 时不启动任何 Git 进程。
pub(crate) fn read_worktrees(repo: &Path) -> Result<(Vec<WorktreeInfo>, Vec<String>), String> {
    // 普通仓库的 .git 是目录：没有 .git/worktrees 就一定没有 linked worktree，直接跳过。
    // .git 是文件时（本身是 worktree 或子模块）无法这样判断，交给 Git。
    let dot_git = repo.join(".git");
    if dot_git.is_dir() && !has_entries(&dot_git.join("worktrees")) {
        return Ok((Vec::new(), Vec::new()));
    }
    let output = run_in(repo, &["worktree", "list", "--porcelain"])?;
    let mut worktrees = parse_worktree_list(&String::from_utf8_lossy(&output.stdout));
    let self_key = crate::paths::path_key(repo);
    // 只保留「其他」工作区：当前仓库是主工作区时排除主工作区，
    // 当前仓库本身是 linked worktree 时排除它自己，主工作区作为其他工作区保留。
    worktrees.retain(|tree| crate::paths::path_key(Path::new(&tree.path)) != self_key);
    let mut status_lines = Vec::new();
    for tree in worktrees.iter_mut() {
        if tree.prunable || !Path::new(&tree.path).exists() {
            tree.prunable = true;
            continue;
        }
        if let Ok(output) = run_in(Path::new(&tree.path), &["status", "--porcelain"]) {
            let text = String::from_utf8_lossy(&output.stdout).into_owned();
            let lines: Vec<&str> = text.lines().filter(|line| !line.is_empty()).collect();
            tree.changed_files = Some(lines.len() as u32);
            status_lines.push(format!("{}\n{}", tree.path, lines.join("\n")));
        }
    }
    Ok((worktrees, status_lines))
}

fn has_entries(dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .map(|mut entries| entries.next().is_some())
        .unwrap_or(false)
}

/// 解析 `git worktree list --porcelain`。每个工作区一段，段之间空行分隔。
/// 返回值包含主工作区（第一段）；bare 仓库没有工作区，不计入。
pub(crate) fn parse_worktree_list(text: &str) -> Vec<WorktreeInfo> {
    let mut trees = Vec::new();
    let mut current: Option<WorktreeInfo> = None;
    let mut current_bare = false;
    for raw in text.lines() {
        let line = raw.trim_end_matches('\r');
        if let Some(path) = line.strip_prefix("worktree ") {
            if let Some(tree) = current.take() {
                if !current_bare {
                    trees.push(tree);
                }
            }
            current_bare = false;
            current = Some(WorktreeInfo {
                path: path.to_string(),
                branch: None,
                head: String::new(),
                changed_files: None,
                prunable: false,
            });
            continue;
        }
        let Some(tree) = current.as_mut() else {
            continue;
        };
        if let Some(head) = line.strip_prefix("HEAD ") {
            tree.head = head.to_string();
        } else if let Some(branch) = line.strip_prefix("branch ") {
            tree.branch = Some(branch.trim_start_matches("refs/heads/").to_string());
        } else if line == "prunable" || line.starts_with("prunable ") {
            tree.prunable = true;
        } else if line == "bare" {
            current_bare = true;
        }
    }
    if let Some(tree) = current {
        if !current_bare {
            trees.push(tree);
        }
    }
    trees
}

/// 简单通配：`*` 匹配任意长度字符（含 `/`），其余字符逐个比较，大小写敏感（与 Git 分支名一致）。
pub fn branch_matches(pattern: &str, name: &str) -> bool {
    let pattern = pattern.as_bytes();
    let name = name.as_bytes();
    let (mut p, mut n) = (0usize, 0usize);
    // 最近一个 `*` 的位置，以及它当时对应的 name 位置，用于回溯。
    let mut star: Option<(usize, usize)> = None;
    while n < name.len() {
        if p < pattern.len() && pattern[p] == b'*' {
            star = Some((p, n));
            p += 1;
        } else if p < pattern.len() && pattern[p] == name[n] {
            p += 1;
            n += 1;
        } else if let Some((star_p, star_n)) = star {
            p = star_p + 1;
            n = star_n + 1;
            star = Some((star_p, star_n + 1));
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == b'*' {
        p += 1;
    }
    p == pattern.len()
}

pub fn is_protected(patterns: &[String], branch: &str) -> bool {
    patterns
        .iter()
        .any(|pattern| branch_matches(pattern, branch))
}

/// 稳定的 64 位 FNV-1a 哈希。快照要持久化，不能用标准库每次进程都可能不同的哈希。
pub(crate) struct Fingerprint(u64);

impl Fingerprint {
    pub fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    /// 写入一个字段，并追加分隔字节，避免 "ab"+"c" 与 "a"+"bc" 得到相同结果。
    pub fn field(&mut self, bytes: &[u8]) {
        for &byte in bytes.iter().chain(std::iter::once(&0xff)) {
            self.0 ^= u64::from(byte);
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
    }

    pub fn finish(&self) -> String {
        format!("{:016x}", self.0)
    }
}

/// 从 porcelain v2 的一行里取出相对路径（重命名时取新路径）。头部行和无法识别的行返回 None。
pub(crate) fn porcelain_path(line: &str) -> Option<String> {
    let (count, rename) = match line.as_bytes().first()? {
        b'1' => (8, false),
        b'2' => (9, true),
        b'u' => (10, false),
        b'?' | b'!' => (1, false),
        _ => return None,
    };
    let (_, rest) = take_fields(line, count)?;
    let path = if rename {
        rest.split_once('\t')
            .map(|(path, _)| path.to_string())
            .unwrap_or(rest)
    } else {
        rest
    };
    Some(unquote_git_path(&path))
}

/// 文件的大小和修改时间。用于发现「已经有改动的文件又被改了」，Git 状态码不会因此变化。
pub(crate) fn file_stamp(repo: &Path, rel: &str) -> String {
    match std::fs::metadata(repo.join(rel)) {
        Ok(meta) => {
            let modified = meta
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_nanos())
                .unwrap_or(0);
            format!("{}:{}", meta.len(), modified)
        }
        Err(_) => "missing".to_string(),
    }
}

fn run_in(repo: &Path, args: &[&str]) -> Result<Output, String> {
    let mut command = git_command();
    command
        .arg("-c")
        .arg("core.quotePath=true")
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(repo)
        .args(args);
    let output = git_output(&mut command)?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(stderr_brief(&output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_branches_and_remote_presence() {
        let text = "refs/heads/main\0*\0origin/main\0ahead 2, behind 1\x001700000000\0aaa\n\
                    refs/heads/feature/x\0 \0\0\x001700000100\0bbb\n\
                    refs/heads/old\0 \0origin/old\0gone\x001600000000\0ccc\n\
                    refs/remotes/origin/main\0 \0\0\x001700000000\0ddd\n";
        let (branches, has_remote) = parse_for_each_ref(text);
        assert!(has_remote);
        assert_eq!(branches.len(), 3);
        assert!(branches[0].is_current);
        assert_eq!(branches[0].oid, "aaa");
        assert_eq!((branches[0].ahead, branches[0].behind), (2, 1));
        assert_eq!(branches[0].unpushed, Some(2));
        assert_eq!(branches[1].name, "feature/x");
        assert_eq!(branches[1].upstream, None);
        assert_eq!(branches[1].unpushed, None, "无上游的分支需另行统计");
        assert!(branches[2].upstream_gone);
        assert_eq!(branches[2].unpushed, None);
        assert_eq!(branches[2].last_commit_at, Some(1_600_000_000));
    }

    #[test]
    fn local_only_repo_has_no_remote() {
        let (branches, has_remote) = parse_for_each_ref("refs/heads/main\0*\0\0\x001\0aaa\n");
        assert!(!has_remote);
        assert_eq!(branches.len(), 1);
    }

    #[test]
    fn parses_worktree_list() {
        let text = "worktree D:/work/app\nHEAD aaa\nbranch refs/heads/main\n\n\
                    worktree D:/work/app-agent\nHEAD bbb\nbranch refs/heads/agent/fix\n\n\
                    worktree D:/work/gone\nHEAD ccc\ndetached\nprunable gitdir file points to non-existent location\n";
        let trees = parse_worktree_list(text);
        assert_eq!(trees.len(), 3);
        assert_eq!(trees[1].branch.as_deref(), Some("agent/fix"));
        assert_eq!(trees[2].branch, None);
        assert!(trees[2].prunable);

        let bare = "worktree D:/srv/app.git\nbare\n\nworktree D:/work/app\nHEAD aaa\nbranch refs/heads/main\n";
        let trees = parse_worktree_list(bare);
        assert_eq!(trees.len(), 1, "bare 仓库本身没有工作区");
        assert_eq!(trees[0].path, "D:/work/app");
    }

    #[test]
    fn wildcard_branch_matching() {
        assert!(branch_matches("main", "main"));
        assert!(!branch_matches("main", "main2"));
        assert!(branch_matches("release/*", "release/1.0"));
        assert!(branch_matches("release/*", "release/1.0/hotfix"));
        assert!(!branch_matches("release/*", "releases/1.0"));
        assert!(branch_matches("*-stable", "v2-stable"));
        assert!(branch_matches("a*b*c", "a-x-b-y-c"));
        assert!(!branch_matches("a*b*c", "a-x-b-y"));
        assert!(branch_matches("*", ""));
        let patterns = vec!["main".to_string(), "release/*".to_string()];
        assert!(is_protected(&patterns, "release/2"));
        assert!(!is_protected(&patterns, "feature/a"));
    }

    #[test]
    fn porcelain_paths() {
        assert_eq!(
            porcelain_path("1 .M N... 100644 100644 100644 a b src/main.rs").as_deref(),
            Some("src/main.rs")
        );
        assert_eq!(
            porcelain_path("2 R. N... 100644 100644 100644 a b R100 new name.txt\told.txt")
                .as_deref(),
            Some("new name.txt")
        );
        assert_eq!(
            porcelain_path("? \"\\344\\270\\255.txt\"").as_deref(),
            Some("中.txt")
        );
        assert_eq!(porcelain_path("# branch.head main"), None);
    }

    #[test]
    fn fingerprint_is_stable_and_field_separated() {
        let mut first = Fingerprint::new();
        first.field(b"ab");
        first.field(b"c");
        let mut second = Fingerprint::new();
        second.field(b"a");
        second.field(b"bc");
        assert_ne!(first.finish(), second.finish());
        let mut again = Fingerprint::new();
        again.field(b"ab");
        again.field(b"c");
        assert_eq!(first.finish(), again.finish());
    }
}
