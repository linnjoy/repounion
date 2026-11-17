//! 通过本机 Git 读取状态、刷新远端引用。
//!
//! 「刷新状态」只跑 `git status`，不访问网络。
//! 「Fetch 全部」只执行 `git fetch`，不合并、不拉取到本地分支。

use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::{Condvar, Mutex};

use serde::Serialize;

use crate::parallel::map_parallel;
use crate::paths::repo_name;

/// 同时查询的仓库数量上限。
pub const GIT_CONCURRENCY: usize = 8;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Tone {
    /// 有本地改动，且不是冲突或分叉。
    Dirty,
    /// 工作区干净，但超前或落后远端。
    Unsynced,
    /// 存在冲突，或与远端分叉。
    Conflict,
    /// 干净且与远端一致。
    Clean,
    /// 干净，但没有上游分支。
    NoUpstream,
    /// 无法读取。
    Error,
}

pub fn badge_label(tone: Tone) -> &'static str {
    match tone {
        Tone::Dirty => "有改动",
        Tone::Unsynced => "未同步",
        Tone::Conflict => "冲突",
        Tone::Clean => "干净",
        Tone::NoUpstream => "无上游",
        Tone::Error => "无法读取",
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedStatus {
    pub branch: String,
    pub detached: bool,
    pub oid: String,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub changed_files: u32,
    pub conflict_files: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RepoStatus {
    pub path: String,
    pub name: String,
    pub ok: bool,
    pub error: Option<String>,
    pub branch: String,
    pub detached: bool,
    pub upstream: Option<String>,
    pub has_upstream: bool,
    pub ahead: u32,
    pub behind: u32,
    pub diverged: bool,
    pub changed_files: u32,
    pub conflict_files: u32,
    pub workspace_label: String,
    pub remote_label: String,
    pub tone: Tone,
}

impl RepoStatus {
    fn from_parsed(path: &str, parsed: ParsedStatus) -> Self {
        let has_upstream = parsed.upstream.is_some();
        let diverged = has_upstream && parsed.ahead > 0 && parsed.behind > 0;
        let dirty = parsed.changed_files > 0;
        let conflict = parsed.conflict_files > 0;
        // 色条优先级：冲突或分叉 > 有改动 > 没有上游 > 干净但未对齐 > 干净且已同步。
        let tone = if conflict || diverged {
            Tone::Conflict
        } else if dirty {
            Tone::Dirty
        } else if !has_upstream {
            Tone::NoUpstream
        } else if parsed.ahead > 0 || parsed.behind > 0 {
            Tone::Unsynced
        } else {
            Tone::Clean
        };

        let workspace_label = if parsed.conflict_files > 0 {
            format!("冲突，{} 个文件", parsed.conflict_files)
        } else if parsed.changed_files > 0 {
            format!("{} 个文件已修改", parsed.changed_files)
        } else {
            "工作区干净".to_string()
        };

        let remote_label = if !has_upstream {
            "没有上游分支".to_string()
        } else if diverged {
            format!("分叉 · 超前 {} 落后 {}", parsed.ahead, parsed.behind)
        } else if parsed.ahead > 0 {
            format!("超前 {} 个提交", parsed.ahead)
        } else if parsed.behind > 0 {
            format!("落后 {} 个提交", parsed.behind)
        } else {
            "与远端一致".to_string()
        };

        let branch = if parsed.detached {
            let short: String = parsed.oid.chars().take(7).collect();
            if short.is_empty() {
                "游离 HEAD".to_string()
            } else {
                format!("游离 {short}")
            }
        } else if parsed.branch.is_empty() {
            "—".to_string()
        } else {
            parsed.branch
        };

        Self {
            path: path.to_string(),
            name: repo_name(path),
            ok: true,
            error: None,
            branch,
            detached: parsed.detached,
            upstream: parsed.upstream,
            has_upstream,
            ahead: parsed.ahead,
            behind: parsed.behind,
            diverged,
            changed_files: parsed.changed_files,
            conflict_files: parsed.conflict_files,
            workspace_label,
            remote_label,
            tone,
        }
    }

    fn failed(path: &str, message: String) -> Self {
        Self {
            path: path.to_string(),
            name: repo_name(path),
            ok: false,
            error: Some(message),
            branch: "—".to_string(),
            detached: false,
            upstream: None,
            has_upstream: false,
            ahead: 0,
            behind: 0,
            diverged: false,
            changed_files: 0,
            conflict_files: 0,
            workspace_label: "无法读取".to_string(),
            remote_label: "—".to_string(),
            tone: Tone::Error,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StatusReport {
    pub git_missing: bool,
    pub items: Vec<RepoStatus>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FetchFailure {
    pub path: String,
    pub name: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FetchReport {
    pub git_missing: bool,
    pub failures: Vec<FetchFailure>,
    pub items: Vec<RepoStatus>,
}

pub fn query_statuses(paths: &[String]) -> StatusReport {
    if paths.is_empty() {
        return StatusReport {
            git_missing: false,
            items: Vec::new(),
        };
    }
    if !git_available() {
        return StatusReport {
            git_missing: true,
            items: paths
                .iter()
                .map(|path| RepoStatus::failed(path, "未找到 Git，请先安装并加入 PATH".to_string()))
                .collect(),
        };
    }
    let items = map_parallel(paths.to_vec(), GIT_CONCURRENCY, |path| status_of(&path));
    StatusReport {
        git_missing: false,
        items,
    }
}

pub fn fetch_all(paths: &[String]) -> FetchReport {
    if paths.is_empty() {
        return FetchReport {
            git_missing: false,
            failures: Vec::new(),
            items: Vec::new(),
        };
    }
    if !git_available() {
        return FetchReport {
            git_missing: true,
            failures: Vec::new(),
            items: paths
                .iter()
                .map(|path| RepoStatus::failed(path, "未找到 Git，请先安装并加入 PATH".to_string()))
                .collect(),
        };
    }
    let rows = map_parallel(paths.to_vec(), GIT_CONCURRENCY, |path| {
        let fetch_error = run_fetch(Path::new(&path)).err();
        let status = status_of(&path);
        (path, fetch_error, status)
    });
    let mut failures = Vec::new();
    let mut items = Vec::with_capacity(rows.len());
    for (path, fetch_error, status) in rows {
        if let Some(message) = fetch_error {
            failures.push(FetchFailure {
                name: repo_name(&path),
                path: path.clone(),
                message,
            });
        }
        items.push(status);
    }
    FetchReport {
        git_missing: false,
        failures,
        items,
    }
}

pub fn status_of(path: &str) -> RepoStatus {
    match run_status(Path::new(path)) {
        Ok(parsed) => RepoStatus::from_parsed(path, parsed),
        Err(message) => RepoStatus::failed(path, message),
    }
}

/// Windows 上 `git.exe` 是控制台程序。不带 `CREATE_NO_WINDOW` 时，
/// 每查一个仓库都会闪出一个黑色命令行窗口。
pub(crate) fn hide_console_window(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        let _ = command;
    }
}

fn git_available() -> bool {
    let mut command = Command::new("git");
    hide_console_window(&mut command);
    let _slot = GitSlot::acquire();
    command
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub(crate) fn git_command() -> Command {
    let mut command = Command::new("git");
    hide_console_window(&mut command);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "Never")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_IMPLICIT_WORK_TREE")
        .env_remove("GIT_EXTERNAL_DIFF");
    command
}

/// 跑一条已经配好的 Git 命令，并占用一个全局并发名额。
pub(crate) fn git_output(command: &mut Command) -> Result<Output, String> {
    let _slot = GitSlot::acquire();
    command.output().map_err(command_error)
}

/// 同 `git_output`，但把 `input` 写进标准输入。调用前不用自己设置 stdin。
pub(crate) fn git_output_stdin(command: &mut Command, input: &[u8]) -> Result<Output, String> {
    use std::io::Write;
    command.stdin(Stdio::piped());
    let _slot = GitSlot::acquire();
    let mut child = command.spawn().map_err(command_error)?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(input).map_err(command_error)?;
        // 关掉管道，Git 才能读到 EOF。
        drop(stdin);
    }
    child.wait_with_output().map_err(command_error)
}

struct GitSlot;

impl GitSlot {
    fn acquire() -> Self {
        let mut guard = GIT_IN_FLIGHT.lock().expect("Git 并发锁已损坏");
        while *guard >= GIT_CONCURRENCY {
            guard = GIT_SLOT_CV.wait(guard).expect("Git 并发锁已损坏");
        }
        *guard += 1;
        Self
    }
}

impl Drop for GitSlot {
    fn drop(&mut self) {
        let mut guard = GIT_IN_FLIGHT.lock().expect("Git 并发锁已损坏");
        *guard = guard.saturating_sub(1);
        GIT_SLOT_CV.notify_one();
    }
}

static GIT_IN_FLIGHT: Mutex<usize> = Mutex::new(0);
static GIT_SLOT_CV: Condvar = Condvar::new();

fn run_status(path: &Path) -> Result<ParsedStatus, String> {
    if !path.exists() {
        return Err("路径不存在".into());
    }
    // `--no-optional-locks` 尽量不写 index；默认的未跟踪模式把整个未跟踪目录算作一项。
    let output = git_output(
        git_command()
            .arg("-c")
            .arg("core.quotePath=true")
            .arg("--no-optional-locks")
            .arg("-C")
            .arg(path)
            .args(["status", "--porcelain=v2", "--branch"]),
    )?;
    if !output.status.success() {
        return Err(stderr_brief(&output));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    parse_porcelain_v2(&text)
}

fn run_fetch(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err("路径不存在".into());
    }
    // `--all` 更新每个远端的引用；不带 merge，本地分支保持原样。
    let output = git_output(
        git_command()
            .arg("-C")
            .arg(path)
            .args(["fetch", "--all", "--prune", "--quiet"]),
    )?;
    if output.status.success() {
        Ok(())
    } else {
        Err(stderr_brief(&output))
    }
}

pub(crate) fn command_error(err: std::io::Error) -> String {
    if err.kind() == std::io::ErrorKind::NotFound {
        "未找到 Git，请先安装并加入 PATH".to_string()
    } else {
        format!("无法执行 Git：{err}")
    }
}

pub(crate) fn stderr_brief(output: &Output) -> String {
    let text = String::from_utf8_lossy(&output.stderr);
    let line = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("");
    let line = line.trim();
    if line.is_empty() {
        return "git 命令失败".to_string();
    }
    let shortened: String = line.chars().take(180).collect();
    if line.chars().count() > 180 {
        format!("{shortened}…")
    } else {
        shortened
    }
}

pub fn parse_porcelain_v2(text: &str) -> Result<ParsedStatus, String> {
    let mut branch = String::new();
    let mut detached = false;
    let mut oid = String::new();
    let mut upstream = None;
    let mut ahead = 0;
    let mut behind = 0;
    let mut changed_files = 0;
    let mut conflict_files = 0;

    for raw in text.split('\n') {
        let line = raw.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("# branch.head ") {
            if rest == "(detached)" {
                detached = true;
            } else {
                branch = rest.to_string();
            }
        } else if let Some(rest) = line.strip_prefix("# branch.upstream ") {
            if !rest.is_empty() {
                upstream = Some(rest.to_string());
            }
        } else if let Some(rest) = line.strip_prefix("# branch.ab ") {
            if let Some((next_ahead, next_behind)) = parse_ab(rest) {
                ahead = next_ahead;
                behind = next_behind;
            }
        } else if let Some(rest) = line.strip_prefix("# branch.oid ") {
            if rest != "(initial)" {
                oid = rest.to_string();
            }
        } else if line.starts_with("1 ") || line.starts_with("2 ") || line.starts_with("? ") {
            changed_files += 1;
        } else if line.starts_with("u ") {
            conflict_files += 1;
        }
    }

    Ok(ParsedStatus {
        branch,
        detached,
        oid,
        upstream,
        ahead,
        behind,
        changed_files,
        conflict_files,
    })
}

fn parse_ab(text: &str) -> Option<(u32, u32)> {
    let mut parts = text.split_whitespace();
    let ahead: i32 = parts.next()?.parse().ok()?;
    let behind: i32 = parts.next()?.parse().ok()?;
    Some((ahead.unsigned_abs(), behind.unsigned_abs()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;

    #[test]
    fn parses_clean_synced() {
        let text = "\
# branch.oid 0123456789abcdef0123456789abcdef01234567
# branch.head main
# branch.upstream origin/main
# branch.ab +0 -0
";
        let status = RepoStatus::from_parsed("D:/work/app", parse_porcelain_v2(text).unwrap());
        assert_eq!(status.branch, "main");
        assert!(status.has_upstream);
        assert_eq!(status.workspace_label, "工作区干净");
        assert_eq!(status.remote_label, "与远端一致");
        assert_eq!(status.tone, Tone::Clean);
    }

    #[test]
    fn parses_conflict_and_divergence_ahead_of_plain_edits() {
        let text = "\
# branch.oid abcdef1234567890abcdef1234567890abcdef12
# branch.head feature/sync
# branch.upstream origin/feature/sync
# branch.ab +1 -3
1 .M N... 100644 100644 100644 a b src/main.rs
u UU N... 100644 100644 100644 100644 a b c d src/conflict.rs
? notes.txt
";
        let status = RepoStatus::from_parsed("/work/app", parse_porcelain_v2(text).unwrap());
        assert_eq!(status.changed_files, 2);
        assert_eq!(status.conflict_files, 1);
        assert!(status.diverged);
        assert_eq!(status.workspace_label, "冲突，1 个文件");
        assert_eq!(status.remote_label, "分叉 · 超前 1 落后 3");
        assert_eq!(status.tone, Tone::Conflict);
    }

    #[test]
    fn dirty_wins_over_behind_and_missing_upstream() {
        let behind = "\
# branch.oid aaa
# branch.head main
# branch.upstream origin/main
# branch.ab +0 -2
1 .M N... 100644 100644 100644 a b a.txt
1 M. N... 100644 100644 100644 a b b.txt
2 R. N... 100644 100644 100644 a b R100 new.txt\told.txt
";
        let status = RepoStatus::from_parsed("/work/app", parse_porcelain_v2(behind).unwrap());
        assert_eq!(status.changed_files, 3);
        assert_eq!(status.workspace_label, "3 个文件已修改");
        assert_eq!(status.remote_label, "落后 2 个提交");
        assert_eq!(status.tone, Tone::Dirty);

        let no_upstream = "\
# branch.oid abc
# branch.head develop
1 .M N... 100644 100644 100644 a b a.txt
";
        let status = RepoStatus::from_parsed("/work/app", parse_porcelain_v2(no_upstream).unwrap());
        assert!(!status.has_upstream);
        assert_eq!(status.remote_label, "没有上游分支");
        assert_eq!(status.tone, Tone::Dirty);
    }

    #[test]
    fn clean_no_upstream_is_gray_and_ahead_is_blue() {
        let no_upstream = "\
# branch.oid abcdef1234567890abcdef1234567890abcdef12
# branch.head develop
";
        let status = RepoStatus::from_parsed("/work/app", parse_porcelain_v2(no_upstream).unwrap());
        assert_eq!(status.tone, Tone::NoUpstream);
        assert_eq!(status.workspace_label, "工作区干净");
        assert_eq!(status.remote_label, "没有上游分支");

        let ahead = "\
# branch.oid abc
# branch.head release/1.0
# branch.upstream origin/release/1.0
# branch.ab +2 -0
";
        let status = RepoStatus::from_parsed("/work/app", parse_porcelain_v2(ahead).unwrap());
        assert_eq!(status.tone, Tone::Unsynced);
        assert_eq!(status.remote_label, "超前 2 个提交");
        assert_eq!(status.branch, "release/1.0");
    }

    #[test]
    fn detached_head_shows_short_oid() {
        let text = "\
# branch.oid abcdef1234567890abcdef1234567890abcdef12
# branch.head (detached)
";
        let status = RepoStatus::from_parsed("/work/app", parse_porcelain_v2(text).unwrap());
        assert!(status.detached);
        assert_eq!(status.branch, "游离 abcdef1");
        assert_eq!(status.tone, Tone::NoUpstream);
    }

    fn git_ok() -> bool {
        let mut command = Command::new("git");
        hide_console_window(&mut command);
        command
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    pub(crate) fn git(dir: &Path, args: &[&str]) {
        let mut command = Command::new("git");
        hide_console_window(&mut command);
        let status = command
            .current_dir(dir)
            .args(args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GCM_INTERACTIVE", "Never")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_IMPLICIT_WORK_TREE")
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} 失败");
    }

    #[test]
    fn live_status_sees_local_edits_without_fetch() {
        if !git_ok() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-b", "main"]);
        git(dir.path(), &["config", "user.email", "t@example.com"]);
        git(dir.path(), &["config", "user.name", "tester"]);
        fs::write(dir.path().join("a.txt"), "v1\n").unwrap();
        git(dir.path(), &["add", "a.txt"]);
        git(dir.path(), &["commit", "-m", "init"]);

        let clean = status_of(&dir.path().to_string_lossy());
        assert!(clean.ok, "{:?}", clean.error);
        assert_eq!(clean.workspace_label, "工作区干净");
        assert_eq!(clean.remote_label, "没有上游分支");
        assert_eq!(clean.tone, Tone::NoUpstream);

        fs::write(dir.path().join("a.txt"), "v2\n").unwrap();
        fs::write(dir.path().join("new.txt"), "x\n").unwrap();
        let dirty = status_of(&dir.path().to_string_lossy());
        assert_eq!(dirty.changed_files, 2);
        assert_eq!(dirty.workspace_label, "2 个文件已修改");
        assert_eq!(dirty.tone, Tone::Dirty);
    }

    #[test]
    fn fetch_updates_remote_refs_without_merging() {
        if !git_ok() {
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let origin = root.path().join("origin");
        fs::create_dir(&origin).unwrap();
        git(&origin, &["init", "-b", "main"]);
        git(&origin, &["config", "user.email", "t@example.com"]);
        git(&origin, &["config", "user.name", "tester"]);
        fs::write(origin.join("a.txt"), "v1\n").unwrap();
        git(&origin, &["add", "a.txt"]);
        git(&origin, &["commit", "-m", "init"]);

        git(root.path(), &["clone", "origin", "local"]);
        let local = root.path().join("local");
        let local_file = fs::read_to_string(local.join("a.txt")).unwrap();

        fs::write(origin.join("a.txt"), "v2\n").unwrap();
        git(&origin, &["add", "a.txt"]);
        git(&origin, &["commit", "-m", "v2"]);

        let before = status_of(&local.to_string_lossy());
        assert_eq!(before.behind, 0, "刷新状态不应自己去访问远端");
        assert_eq!(before.tone, Tone::Clean);

        let report = fetch_all(&[local.to_string_lossy().into_owned()]);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(report.items[0].behind, 1);
        assert_eq!(report.items[0].remote_label, "落后 1 个提交");
        assert_eq!(report.items[0].tone, Tone::Unsynced);
        let after = fs::read_to_string(local.join("a.txt")).unwrap();
        assert_eq!(after, local_file, "Fetch 不应改写工作区文件");
    }
}
