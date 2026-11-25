//! 通过本机 Git 读取状态、刷新远端引用。
//!
//! 「刷新状态」只跑 `git status`，不访问网络。
//! 「Fetch 全部」只执行 `git fetch`，不合并、不拉取到本地分支。

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::insight::{
    file_stamp, is_protected, porcelain_path, read_branches, read_worktrees, BranchInfo,
    Fingerprint, WorktreeInfo,
};
use crate::parallel::map_parallel;
use crate::paths::repo_name;

/// 同时查询的仓库数量上限。
pub const GIT_CONCURRENCY: usize = 8;

/// 本地只读 Git 命令（status、cat-file、diff 等）的超时。
/// 正常仓库远低于这个值；超时通常是仓库被锁、磁盘异常或 Git 钩子卡住。
pub const LOCAL_TIMEOUT: Duration = Duration::from_secs(30);

/// 联网命令（fetch）的超时。超过后终止该仓库的 Git 进程，不影响其他仓库。
pub const NETWORK_TIMEOUT: Duration = Duration::from_secs(120);

/// 没有用户自定义 SSH 程序时使用的非交互 SSH 命令。
/// BatchMode 让主机指纹确认、密钥口令等提示直接失败，而不是在后台无限等待。
const BATCH_SSH_COMMAND: &str = "ssh -o BatchMode=yes -o ConnectTimeout=15";

/// 取消标志。前端点「取消」时置位，正在运行的 Git 进程会被终止，尚未开始的直接跳过。
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// 取消时返回的错误文字，前端据此区分「失败」和「已取消」。
pub const CANCELLED_MESSAGE: &str = "已取消";

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
    /// 未跟踪的条目数（已包含在 changed_files 里）。默认模式下整个未跟踪目录算一项。
    pub untracked_files: u32,
    /// `--show-stash` 输出的 stash 数量。
    pub stash_count: u32,
    /// 改动条目的原始行（1/2/u/?），用于计算改动指纹。
    pub entries: Vec<String>,
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
    /// 未跟踪的条目数（已包含在 changed_files 里）。
    pub untracked_files: u32,
    pub stash_count: u32,
    /// 所有本地分支（含当前分支）的推送情况。
    pub branches: Vec<BranchInfo>,
    /// 除当前分支外，还有未推送提交的分支数量。
    pub other_unpushed_branches: u32,
    /// 是否存在任何远端引用。没有远端时不统计「未推送」。
    pub has_remote: bool,
    /// 当前仓库之外的其他工作区（linked worktree，或当前是 worktree 时的主工作区）。
    pub worktrees: Vec<WorktreeInfo>,
    pub head_oid: String,
    pub last_commit_at: Option<i64>,
    /// 当前分支命中工作区的保护分支规则。
    pub on_protected_branch: bool,
    /// 在保护分支上有未提交改动或未推送提交，需要警告。
    pub protected_risk: bool,
    /// 本地状态指纹：HEAD、本地分支、stash、改动文件及其修改时间、其他工作区的改动。
    /// 不含远端引用，所以 Fetch 不会改变它。
    pub fingerprint: String,
    /// 指纹与上次「标记已查看」时不同。由调用方根据快照填写。
    pub changed_since_ack: bool,
    /// 附加信息读取失败的说明。主状态仍然有效。
    pub notes: Vec<String>,
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
            untracked_files: parsed.untracked_files,
            stash_count: parsed.stash_count,
            branches: Vec::new(),
            other_unpushed_branches: 0,
            has_remote: false,
            worktrees: Vec::new(),
            head_oid: parsed.oid,
            last_commit_at: None,
            on_protected_branch: false,
            protected_risk: false,
            fingerprint: String::new(),
            changed_since_ack: false,
            notes: Vec::new(),
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
            untracked_files: 0,
            stash_count: 0,
            branches: Vec::new(),
            other_unpushed_branches: 0,
            has_remote: false,
            worktrees: Vec::new(),
            head_oid: String::new(),
            last_commit_at: None,
            on_protected_branch: false,
            protected_risk: false,
            fingerprint: String::new(),
            changed_since_ack: false,
            notes: Vec::new(),
        }
    }

    /// 补充分支、worktree、保护分支和指纹。任何一项失败只记入 notes，不影响主状态。
    fn enrich(&mut self, repo: &Path, branch_name: &str, entries: &[String], protected: &[String]) {
        match read_branches(repo) {
            Ok((branches, has_remote)) => {
                self.has_remote = has_remote;
                self.other_unpushed_branches = branches
                    .iter()
                    .filter(|branch| !branch.is_current && branch.unpushed.unwrap_or(0) > 0)
                    .count() as u32;
                self.last_commit_at = branches
                    .iter()
                    .find(|branch| branch.is_current)
                    .and_then(|branch| branch.last_commit_at);
                self.branches = branches;
            }
            Err(message) => self.notes.push(format!("读取分支失败：{message}")),
        }

        let mut worktree_lines = Vec::new();
        match read_worktrees(repo) {
            Ok((worktrees, lines)) => {
                self.worktrees = worktrees;
                worktree_lines = lines;
            }
            Err(message) => self.notes.push(format!("读取 worktree 失败：{message}")),
        }

        // 游离 HEAD 不属于任何分支，不做保护分支判断。
        if !self.detached && !branch_name.is_empty() {
            self.on_protected_branch = is_protected(protected, branch_name);
        }
        let current_unpushed = self
            .branches
            .iter()
            .find(|branch| branch.is_current)
            .and_then(|branch| branch.unpushed)
            .unwrap_or(0);
        self.protected_risk = self.on_protected_branch
            && (self.changed_files > 0 || self.conflict_files > 0 || current_unpushed > 0);

        self.fingerprint = self.compute_fingerprint(repo, entries, &worktree_lines);
    }

    fn compute_fingerprint(
        &self,
        repo: &Path,
        entries: &[String],
        worktree_lines: &[String],
    ) -> String {
        let mut hash = Fingerprint::new();
        hash.field(self.head_oid.as_bytes());
        hash.field(self.branch.as_bytes());
        hash.field(self.stash_count.to_string().as_bytes());
        // 本地分支取名称和提交号：任何分支有新提交、被重置或新建删除都会变化。
        // 远端引用不参与，Fetch 不会让仓库变成「有新变化」。
        for branch in &self.branches {
            hash.field(branch.name.as_bytes());
            hash.field(branch.oid.as_bytes());
        }
        let mut sorted: Vec<&String> = entries.iter().collect();
        sorted.sort();
        for line in sorted {
            hash.field(line.as_bytes());
            // 状态行不会因「已修改的文件再次被修改」而变化，补上文件大小和修改时间。
            if let Some(rel) = porcelain_path(line) {
                hash.field(file_stamp(repo, &rel).as_bytes());
            }
        }
        for lines in worktree_lines {
            hash.field(lines.as_bytes());
        }
        hash.finish()
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
    /// 本次 Fetch 是否被用户取消。取消后已完成的仓库结果仍然有效。
    pub cancelled: bool,
    pub failures: Vec<FetchFailure>,
    pub items: Vec<RepoStatus>,
}

/// Fetch 过程中每完成一个仓库推送一次，前端逐行更新。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FetchProgress {
    pub item: RepoStatus,
    /// 该仓库 fetch 失败、超时或被取消时的原因；本地状态仍会重新读取。
    pub fetch_error: Option<String>,
}

pub fn query_statuses(paths: &[String]) -> StatusReport {
    query_statuses_with(paths, &[], |_| {})
}

/// 读取本地状态。每完成一个仓库调用一次 `on_item`，调用顺序不保证与 `paths` 一致；
/// 返回值中的 `items` 保持与 `paths` 相同的顺序。
/// `on_item` 可以修改结果（例如填写「有新变化」），修改会保留在返回值里。
/// `protected` 是工作区的保护分支规则。
pub fn query_statuses_with<F>(paths: &[String], protected: &[String], on_item: F) -> StatusReport
where
    F: Fn(&mut RepoStatus) + Sync,
{
    if paths.is_empty() {
        return StatusReport {
            git_missing: false,
            items: Vec::new(),
        };
    }
    if !git_available() {
        return StatusReport {
            git_missing: true,
            items: missing_git_items(paths),
        };
    }
    let items = map_parallel(paths.to_vec(), GIT_CONCURRENCY, |path| {
        let mut status = status_of(&path, protected);
        on_item(&mut status);
        status
    });
    StatusReport {
        git_missing: false,
        items,
    }
}

pub fn fetch_all(paths: &[String]) -> FetchReport {
    fetch_all_with(paths, &[], &CancelToken::new(), |_| {})
}

/// 对每个仓库 fetch 后重新读取本地状态。
/// 某个仓库失败、超时或被取消不会影响其他仓库；取消后尚未开始的仓库跳过 fetch，只读本地状态。
pub fn fetch_all_with<F>(
    paths: &[String],
    protected: &[String],
    cancel: &CancelToken,
    on_item: F,
) -> FetchReport
where
    F: Fn(&mut FetchProgress) + Sync,
{
    if paths.is_empty() {
        return FetchReport {
            git_missing: false,
            cancelled: false,
            failures: Vec::new(),
            items: Vec::new(),
        };
    }
    if !git_available() {
        return FetchReport {
            git_missing: true,
            cancelled: false,
            failures: Vec::new(),
            items: missing_git_items(paths),
        };
    }
    let rows = map_parallel(paths.to_vec(), GIT_CONCURRENCY, |path| {
        let fetch_error = run_fetch(Path::new(&path), cancel).err();
        let status = status_of(&path, protected);
        let mut progress = FetchProgress {
            item: status,
            fetch_error,
        };
        on_item(&mut progress);
        (path, progress.fetch_error, progress.item)
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
        cancelled: cancel.is_cancelled(),
        failures,
        items,
    }
}

fn missing_git_items(paths: &[String]) -> Vec<RepoStatus> {
    paths
        .iter()
        .map(|path| RepoStatus::failed(path, "未找到 Git，请先安装并加入 PATH".to_string()))
        .collect()
}

/// 读取一个仓库的完整本地状态。`protected` 为工作区的保护分支规则。
pub fn status_of(path: &str, protected: &[String]) -> RepoStatus {
    let repo = Path::new(path);
    match run_status(repo) {
        Ok(parsed) => {
            let branch_name = parsed.branch.clone();
            let entries = parsed.entries.clone();
            let mut status = RepoStatus::from_parsed(path, parsed);
            status.enrich(repo, &branch_name, &entries, protected);
            status
        }
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
    let mut command = git_command();
    command.arg("--version");
    run_git(&mut command, None, LOCAL_TIMEOUT, None)
        .map(|output| output.status.success())
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

/// 跑一条已经配好的本地 Git 命令，占用一个全局并发名额，带本地超时。
pub(crate) fn git_output(command: &mut Command) -> Result<Output, String> {
    run_git(command, None, LOCAL_TIMEOUT, None)
}

/// 同 `git_output`，但把 `input` 写进标准输入。调用前不用自己设置 stdin。
pub(crate) fn git_output_stdin(command: &mut Command, input: &[u8]) -> Result<Output, String> {
    run_git(command, Some(input), LOCAL_TIMEOUT, None)
}

/// 运行 Git 子进程：占用并发名额，超时或取消时终止进程。
///
/// 标准输出和标准错误在单独线程里读取，避免输出过多时管道写满导致死锁。
/// 终止进程后不等待读取线程：Git 可能留下仍持有管道的子进程（如 ssh），
/// 等待会把调用方一起卡住；这些线程在管道关闭后自行结束。
fn run_git(
    command: &mut Command,
    input: Option<&[u8]>,
    timeout: Duration,
    cancel: Option<&CancelToken>,
) -> Result<Output, String> {
    let _slot = GitSlot::acquire();
    // 排队等名额期间可能已被取消，这时不再启动进程。
    if cancel.is_some_and(CancelToken::is_cancelled) {
        return Err(CANCELLED_MESSAGE.to_string());
    }

    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    if input.is_some() {
        command.stdin(Stdio::piped());
    }
    let mut child = command.spawn().map_err(command_error)?;

    let writer = match (input, child.stdin.take()) {
        (Some(data), Some(mut stdin)) => {
            let data = data.to_vec();
            // 写完后 stdin 随线程结束被关闭，Git 才能读到 EOF。
            Some(thread::spawn(move || stdin.write_all(&data)))
        }
        _ => None,
    };
    let stdout_reader = spawn_pipe_reader(child.stdout.take());
    let stderr_reader = spawn_pipe_reader(child.stderr.take());

    let deadline = Instant::now() + timeout;
    // 轮询间隔从 1 毫秒逐步放大到 20 毫秒：短命令几乎没有额外延迟，长命令也不空转。
    let mut pause = Duration::from_millis(1);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(err) => {
                kill_child(&mut child);
                return Err(command_error(err));
            }
        }
        if cancel.is_some_and(CancelToken::is_cancelled) {
            kill_child(&mut child);
            return Err(CANCELLED_MESSAGE.to_string());
        }
        if Instant::now() >= deadline {
            kill_child(&mut child);
            return Err(format!("超时（{} 秒），已终止", timeout.as_secs()));
        }
        thread::sleep(pause);
        pause = (pause * 2).min(Duration::from_millis(20));
    };

    if let Some(writer) = writer {
        // Git 提前退出时写入会因管道关闭而失败，此时以 Git 的退出结果为准。
        let _ = writer.join();
    }
    let stdout = stdout_reader.join().unwrap_or_default();
    let stderr = stderr_reader.join().unwrap_or_default();
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

fn spawn_pipe_reader<R>(pipe: Option<R>) -> thread::JoinHandle<Vec<u8>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut buffer = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut buffer);
        }
        buffer
    })
}

fn kill_child(child: &mut std::process::Child) {
    let _ = child.kill();
    // 回收进程句柄，避免僵尸进程。
    let _ = child.wait();
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
            // --show-stash 在头部输出 `# stash N`（Git 2.35+），省掉一次单独的 stash 查询。
            .args(["status", "--porcelain=v2", "--branch", "--show-stash"]),
    )?;
    if !output.status.success() {
        return Err(stderr_brief(&output));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    parse_porcelain_v2(&text)
}

fn run_fetch(path: &Path, cancel: &CancelToken) -> Result<(), String> {
    if cancel.is_cancelled() {
        return Err(CANCELLED_MESSAGE.to_string());
    }
    if !path.exists() {
        return Err("路径不存在".into());
    }
    // `--all` 更新每个远端的引用；不带 merge，本地分支保持原样。
    let mut command = git_command();
    command
        .arg("-C")
        .arg(path)
        .args(["fetch", "--all", "--prune", "--quiet"]);
    if !has_custom_ssh(path) {
        command.env("GIT_SSH_COMMAND", BATCH_SSH_COMMAND);
    }
    let output = run_git(&mut command, None, NETWORK_TIMEOUT, Some(cancel))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(stderr_brief(&output))
    }
}

/// 用户是否自己指定了 SSH 程序（如 plink、Windows 自带 OpenSSH）。
/// 指定了就不覆盖，否则会破坏用户已有的密钥和代理配置。
/// `GIT_SSH_COMMAND` 环境变量优先级高于 `core.sshCommand`，所以两处都要检查。
fn has_custom_ssh(path: &Path) -> bool {
    if std::env::var_os("GIT_SSH_COMMAND").is_some() || std::env::var_os("GIT_SSH").is_some() {
        return true;
    }
    let mut command = git_command();
    command
        .arg("-C")
        .arg(path)
        .args(["config", "--get", "core.sshCommand"]);
    match git_output(&mut command) {
        Ok(output) => output.status.success() && !output.stdout.trim_ascii().is_empty(),
        // 读不到配置时按「未自定义」处理：BatchMode 只会让交互提示失败，不会改变认证方式。
        Err(_) => false,
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
    let mut untracked_files = 0;
    let mut stash_count = 0;
    let mut entries = Vec::new();

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
        } else if let Some(rest) = line.strip_prefix("# stash ") {
            stash_count = rest.trim().parse().unwrap_or(0);
        } else if line.starts_with("1 ") || line.starts_with("2 ") || line.starts_with("? ") {
            changed_files += 1;
            if line.starts_with("? ") {
                untracked_files += 1;
            }
            entries.push(line.to_string());
        } else if line.starts_with("u ") {
            conflict_files += 1;
            entries.push(line.to_string());
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
        untracked_files,
        stash_count,
        entries,
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

    use super::tests_support::{git, git_ok};

    #[test]
    fn parses_stash_and_untracked_counts() {
        let text = "\
# branch.oid abc
# branch.head main
# stash 3
1 .M N... 100644 100644 100644 a b a.txt
? new.txt
? build/
";
        let parsed = parse_porcelain_v2(text).unwrap();
        assert_eq!(parsed.stash_count, 3);
        assert_eq!(parsed.untracked_files, 2);
        assert_eq!(parsed.changed_files, 3, "未跟踪项仍计入改动总数");
        assert_eq!(parsed.entries.len(), 3);
    }

    #[test]
    fn live_status_reports_branches_stash_worktree_and_protection() {
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
        git(&local, &["config", "user.email", "t@example.com"]);
        git(&local, &["config", "user.name", "tester"]);
        // 另一个从未推送过的本地分支上有 1 个提交。
        git(&local, &["switch", "-c", "feature/x"]);
        fs::write(local.join("b.txt"), "b\n").unwrap();
        git(&local, &["add", "b.txt"]);
        git(&local, &["commit", "-m", "feature"]);
        git(&local, &["switch", "main"]);
        // main 上一份 stash，以及一处未提交改动。
        fs::write(local.join("a.txt"), "stashed\n").unwrap();
        git(&local, &["stash"]);
        fs::write(local.join("a.txt"), "dirty\n").unwrap();
        // agent 常用的 linked worktree，里面有一个未跟踪文件。
        let tree = root.path().join("local-agent");
        git(
            &local,
            &[
                "worktree",
                "add",
                "-b",
                "agent/task",
                &tree.to_string_lossy(),
            ],
        );
        fs::write(tree.join("agent.txt"), "x\n").unwrap();

        let protected = vec!["main".to_string()];
        let status = status_of(&local.to_string_lossy(), &protected);
        assert!(status.ok, "{:?}", status.error);
        assert!(status.notes.is_empty(), "{:?}", status.notes);
        assert!(status.has_remote);
        assert_eq!(status.stash_count, 1);
        let feature = status
            .branches
            .iter()
            .find(|b| b.name == "feature/x")
            .unwrap();
        assert_eq!(feature.unpushed, Some(1));
        assert!(status.other_unpushed_branches >= 1);
        assert_eq!(status.worktrees.len(), 1);
        assert_eq!(status.worktrees[0].branch.as_deref(), Some("agent/task"));
        assert_eq!(status.worktrees[0].changed_files, Some(1));
        assert!(status.on_protected_branch);
        assert!(status.protected_risk, "保护分支上有未提交改动应警告");
        assert!(!status.fingerprint.is_empty());
        assert!(status.last_commit_at.is_some());
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

        let clean = status_of(&dir.path().to_string_lossy(), &[]);
        assert!(clean.ok, "{:?}", clean.error);
        assert_eq!(clean.workspace_label, "工作区干净");
        assert_eq!(clean.remote_label, "没有上游分支");
        assert_eq!(clean.tone, Tone::NoUpstream);

        fs::write(dir.path().join("a.txt"), "v2\n").unwrap();
        fs::write(dir.path().join("new.txt"), "x\n").unwrap();
        let dirty = status_of(&dir.path().to_string_lossy(), &[]);
        assert_eq!(dirty.changed_files, 2);
        assert_eq!(dirty.workspace_label, "2 个文件已修改");
        assert_eq!(dirty.tone, Tone::Dirty);
    }

    /// 一个会运行几秒的子进程，用来验证超时和取消。
    fn slow_command() -> Command {
        let mut command;
        if cfg!(windows) {
            command = Command::new("ping");
            command.args(["-n", "6", "127.0.0.1"]);
        } else {
            command = Command::new("sleep");
            command.arg("5");
        }
        hide_console_window(&mut command);
        command.stdin(Stdio::null());
        command
    }

    #[test]
    fn run_git_kills_process_on_timeout() {
        let started = Instant::now();
        let result = run_git(&mut slow_command(), None, Duration::from_millis(300), None);
        let message = result.expect_err("应当超时");
        assert!(message.contains("超时"), "{message}");
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "超时后没有及时返回"
        );
    }

    #[test]
    fn run_git_stops_when_cancelled() {
        let cancel = CancelToken::new();
        let trigger = cancel.clone();
        let canceller = thread::spawn(move || {
            thread::sleep(Duration::from_millis(200));
            trigger.cancel();
        });
        let started = Instant::now();
        let result = run_git(
            &mut slow_command(),
            None,
            Duration::from_secs(30),
            Some(&cancel),
        );
        canceller.join().unwrap();
        assert_eq!(result.unwrap_err(), CANCELLED_MESSAGE);
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "取消后没有及时返回"
        );
    }

    #[test]
    fn cancelled_fetch_still_reads_local_status_and_reports_each_repo() {
        if !git_ok() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-b", "main"]);
        fs::write(dir.path().join("a.txt"), "v1\n").unwrap();

        let cancel = CancelToken::new();
        cancel.cancel();
        let seen = Mutex::new(Vec::new());
        let path = dir.path().to_string_lossy().into_owned();
        let report = fetch_all_with(std::slice::from_ref(&path), &[], &cancel, |progress| {
            seen.lock().unwrap().push(progress.clone());
        });
        assert!(report.cancelled);
        assert_eq!(report.failures.len(), 1);
        assert_eq!(report.failures[0].message, CANCELLED_MESSAGE);
        assert!(report.items[0].ok, "取消 fetch 后仍应读到本地状态");
        assert_eq!(report.items[0].changed_files, 1);
        let seen = seen.into_inner().unwrap();
        assert_eq!(seen.len(), 1, "每个仓库应推送一次进度");
        assert_eq!(seen[0].fetch_error.as_deref(), Some(CANCELLED_MESSAGE));
    }

    #[test]
    fn status_streams_one_item_per_repo() {
        if !git_ok() {
            return;
        }
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        git(first.path(), &["init", "-b", "main"]);
        git(second.path(), &["init", "-b", "main"]);
        let paths = vec![
            first.path().to_string_lossy().into_owned(),
            second.path().to_string_lossy().into_owned(),
        ];
        let seen = Mutex::new(Vec::new());
        let report = query_statuses_with(&paths, &[], |item| {
            seen.lock().unwrap().push(item.path.clone());
        });
        let mut seen = seen.into_inner().unwrap();
        seen.sort();
        let mut expected = paths.clone();
        expected.sort();
        assert_eq!(seen, expected);
        assert_eq!(
            report
                .items
                .iter()
                .map(|item| item.path.clone())
                .collect::<Vec<_>>(),
            paths,
            "汇总结果应保持原顺序"
        );
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

        let before = status_of(&local.to_string_lossy(), &[]);
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

/// 测试共用的 Git 辅助函数。
#[cfg(test)]
pub(crate) mod tests_support {
    use super::hide_console_window;
    use std::path::Path;
    use std::process::{Command, Stdio};

    pub fn git_ok() -> bool {
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

    pub fn git(dir: &Path, args: &[&str]) {
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
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} 失败");
    }
}
