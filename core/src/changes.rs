//! 点开某个仓库之后才读取的改动列表，以及单个文件的差异。
//!
//! 列表来自 `git status --porcelain=v2`（本地与上游则用 `git diff --name-status`）。
//! 左右内容来自 `git cat-file`、暂存区和工作区文件。对齐用 `similar`，不访问网络。

use std::path::{Path, PathBuf};

use serde::Serialize;
use similar::{ChangeTag, DiffOp, TextDiff};

use crate::gitinfo::{git_command, git_output, git_output_stdin, parse_porcelain_v2, stderr_brief};
use crate::paths::is_git_checkout;

/// 文本超过这个字节数就不再打开。1.5 MiB。
pub const TEXT_BYTE_LIMIT: u64 = 1_572_864;
/// 文本超过这个行数就不再打开。
pub const TEXT_LINE_LIMIT: usize = 40_000;
/// 单侧图片超过这个字节数就不再加载。12 MiB。
pub const IMAGE_BYTE_LIMIT: u64 = 12 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompareMode {
    WorktreeHead,
    WorktreeIndex,
    IndexHead,
    LocalUpstream,
}

impl CompareMode {
    fn parse(text: &str) -> Result<Self, String> {
        match text {
            "worktreeHead" => Ok(Self::WorktreeHead),
            "worktreeIndex" => Ok(Self::WorktreeIndex),
            "indexHead" => Ok(Self::IndexHead),
            "localUpstream" => Ok(Self::LocalUpstream),
            _ => Err("未知的比较方式".into()),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::WorktreeHead => "worktreeHead",
            Self::WorktreeIndex => "worktreeIndex",
            Self::IndexHead => "indexHead",
            Self::LocalUpstream => "localUpstream",
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    pub path: String,
    pub old_path: Option<String>,
    /// staged、unstaged、both、untracked、conflict、upstream。
    pub area: String,
    pub code: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChangeList {
    pub mode: String,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub can_compare_upstream: bool,
    pub files: Vec<FileChange>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    /// same、delete、insert。
    pub kind: String,
    pub text: String,
}

/// 一处连续差异。偏移是 UTF-16 码元，和前端字符串下标一致。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffHunk {
    pub from_a: u32,
    pub to_a: u32,
    pub from_b: u32,
    pub to_b: u32,
    pub old_line: u32,
    pub new_line: u32,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HunkSpan {
    pub from_a: u32,
    pub to_a: u32,
    pub from_b: u32,
    pub to_b: u32,
    pub old_line: u32,
    pub new_line: u32,
}

impl From<&DiffHunk> for HunkSpan {
    fn from(hunk: &DiffHunk) -> Self {
        Self {
            from_a: hunk.from_a,
            to_a: hunk.to_a,
            from_b: hunk.from_b,
            to_b: hunk.to_b,
            old_line: hunk.old_line,
            new_line: hunk.new_line,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    pub path: String,
    pub old_path: Option<String>,
    pub mode: String,
    /// text、image、binary、tooLarge、unsupported。
    pub kind: String,
    pub message: Option<String>,
    pub left_text: Option<String>,
    pub right_text: Option<String>,
    pub left_missing: bool,
    pub right_missing: bool,
    pub media_type: Option<String>,
    /// 不含 `data:` 前缀的 base64。
    pub left_image: Option<String>,
    pub right_image: Option<String>,
    pub hunks: Vec<HunkSpan>,
}

/// 超过字节或行数限制时返回说明；未超过则返回 `None`。
pub fn text_limit_reason(bytes: u64, lines: usize) -> Option<&'static str> {
    if bytes > TEXT_BYTE_LIMIT {
        Some("文件超过 1.5 MB，已跳过预览。")
    } else if lines > TEXT_LINE_LIMIT {
        Some("文件超过 4 万行，已跳过预览。")
    } else {
        None
    }
}

pub fn list_changes(repo: &str, mode_text: &str) -> Result<ChangeList, String> {
    let mode = CompareMode::parse(mode_text)?;
    let repo_path = ensure_repo(repo)?;
    let status_text = status_text(&repo_path)?;
    let parsed = parse_porcelain_v2(&status_text)?;
    let can_compare_upstream = parsed.upstream.is_some() && (parsed.ahead > 0 || parsed.behind > 0);
    let files = if mode == CompareMode::LocalUpstream {
        let upstream = parsed
            .upstream
            .clone()
            .filter(|name| !name.is_empty())
            .ok_or("当前分支没有上游。本地与上游只读取已有引用，不会自动 Fetch。")?;
        if parsed.ahead == 0 && parsed.behind == 0 {
            return Err("本地分支与上游一致，没有可查看的提交差异。".into());
        }
        upstream_files(&repo_path, &upstream)?
    } else {
        parse_status_files(&status_text, mode)
    };
    Ok(ChangeList {
        mode: mode.as_str().to_string(),
        upstream: parsed.upstream,
        ahead: parsed.ahead,
        behind: parsed.behind,
        can_compare_upstream,
        files,
    })
}

pub fn load_file_diff(
    repo: &str,
    mode_text: &str,
    path: &str,
    old_path: Option<String>,
) -> Result<FileDiff, String> {
    let mode = CompareMode::parse(mode_text)?;
    let repo_path = ensure_repo(repo)?;
    safe_rel_path(path)?;
    if let Some(old) = &old_path {
        safe_rel_path(old)?;
    }
    let upstream = if mode == CompareMode::LocalUpstream {
        let status_text = status_text(&repo_path)?;
        let parsed = parse_porcelain_v2(&status_text)?;
        let name = parsed
            .upstream
            .filter(|name| !name.is_empty())
            .ok_or("当前分支没有上游。本地与上游只读取已有引用，不会自动 Fetch。")?;
        ensure_ref(&repo_path, &name)?;
        Some(name)
    } else {
        None
    };

    let (left_src, right_src) = side_sources(mode, path, old_path.as_deref(), upstream.as_deref());
    let image = is_image_path(path);
    let left = inspect_side(&repo_path, &left_src)?;
    let right = inspect_side(&repo_path, &right_src)?;

    if left.submodule || right.submodule {
        return Ok(empty_diff(
            path,
            &old_path,
            mode,
            "unsupported",
            Some("这是目录或子模块，当前不支持按单个文件查看。".into()),
            left.missing,
            right.missing,
        ));
    }
    if left.missing && right.missing {
        return Ok(empty_diff(
            path,
            &old_path,
            mode,
            "unsupported",
            Some("两边都没有这个文件。".into()),
            true,
            true,
        ));
    }

    if image {
        if side_too_big(&left, IMAGE_BYTE_LIMIT) || side_too_big(&right, IMAGE_BYTE_LIMIT) {
            return Ok(empty_diff(
                path,
                &old_path,
                mode,
                "tooLarge",
                Some("图片超过 12 MB，已跳过预览。".into()),
                left.missing,
                right.missing,
            ));
        }
    } else if let Some(message) = text_limit_of(&left).or_else(|| text_limit_of(&right)) {
        return Ok(empty_diff(
            path,
            &old_path,
            mode,
            "tooLarge",
            Some(message.into()),
            left.missing,
            right.missing,
        ));
    }

    let left_bytes = read_side(&repo_path, &left_src, &left)?;
    let right_bytes = read_side(&repo_path, &right_src, &right)?;

    if image {
        let media = image_media_type(path).unwrap_or("application/octet-stream");
        return Ok(FileDiff {
            path: path.to_string(),
            old_path,
            mode: mode.as_str().to_string(),
            kind: "image".to_string(),
            message: None,
            left_text: None,
            right_text: None,
            left_missing: left.missing || left_bytes.is_none(),
            right_missing: right.missing || right_bytes.is_none(),
            media_type: Some(media.to_string()),
            left_image: left_bytes.as_deref().map(base64_encode),
            right_image: right_bytes.as_deref().map(base64_encode),
            hunks: Vec::new(),
        });
    }

    let left_raw = left_bytes.unwrap_or_default();
    let right_raw = right_bytes.unwrap_or_default();
    let lines = count_lines(&left_raw).max(count_lines(&right_raw));
    if let Some(message) = text_limit_reason(0, lines) {
        return Ok(empty_diff(
            path,
            &old_path,
            mode,
            "tooLarge",
            Some(message.into()),
            left.missing,
            right.missing,
        ));
    }
    if looks_binary(&left_raw) || looks_binary(&right_raw) {
        return Ok(empty_diff(
            path,
            &old_path,
            mode,
            "binary",
            Some("这是二进制文件，无法按文本预览。".into()),
            left.missing,
            right.missing,
        ));
    }

    let left_text = String::from_utf8_lossy(&left_raw).into_owned();
    let right_text = String::from_utf8_lossy(&right_raw).into_owned();
    let hunks = diff_hunks(&left_text, &right_text);
    Ok(FileDiff {
        path: path.to_string(),
        old_path,
        mode: mode.as_str().to_string(),
        kind: "text".to_string(),
        message: None,
        left_text: Some(left_text),
        right_text: Some(right_text),
        left_missing: left.missing,
        right_missing: right.missing,
        media_type: None,
        left_image: None,
        right_image: None,
        hunks: hunks.iter().map(HunkSpan::from).collect(),
    })
}

fn empty_diff(
    path: &str,
    old_path: &Option<String>,
    mode: CompareMode,
    kind: &str,
    message: Option<String>,
    left_missing: bool,
    right_missing: bool,
) -> FileDiff {
    FileDiff {
        path: path.to_string(),
        old_path: old_path.clone(),
        mode: mode.as_str().to_string(),
        kind: kind.to_string(),
        message,
        left_text: None,
        right_text: None,
        left_missing,
        right_missing,
        media_type: None,
        left_image: None,
        right_image: None,
        hunks: Vec::new(),
    }
}

fn side_too_big(side: &SideMeta, limit: u64) -> bool {
    !side.missing && side.size > limit
}

fn text_limit_of(side: &SideMeta) -> Option<&'static str> {
    if side.missing {
        None
    } else {
        text_limit_reason(side.size, 0)
    }
}

enum SideSource {
    Worktree(String),
    Git(String),
}

struct SideMeta {
    missing: bool,
    submodule: bool,
    size: u64,
}

fn side_sources(
    mode: CompareMode,
    path: &str,
    old_path: Option<&str>,
    upstream: Option<&str>,
) -> (SideSource, SideSource) {
    let head_path = old_path.unwrap_or(path);
    match mode {
        CompareMode::WorktreeHead => (
            SideSource::Git(format!("HEAD:{head_path}")),
            SideSource::Worktree(path.to_string()),
        ),
        CompareMode::WorktreeIndex => (
            SideSource::Git(format!(":0:{path}")),
            SideSource::Worktree(path.to_string()),
        ),
        CompareMode::IndexHead => (
            SideSource::Git(format!("HEAD:{head_path}")),
            SideSource::Git(format!(":0:{path}")),
        ),
        CompareMode::LocalUpstream => {
            let upstream = upstream.unwrap_or("HEAD");
            (
                SideSource::Git(format!("{upstream}:{head_path}")),
                SideSource::Git(format!("HEAD:{path}")),
            )
        }
    }
}

fn inspect_side(repo: &Path, source: &SideSource) -> Result<SideMeta, String> {
    match source {
        SideSource::Worktree(rel) => worktree_meta(repo, rel),
        SideSource::Git(spec) => git_meta(repo, spec),
    }
}

fn read_side(repo: &Path, source: &SideSource, meta: &SideMeta) -> Result<Option<Vec<u8>>, String> {
    if meta.missing || meta.submodule {
        return Ok(None);
    }
    match source {
        SideSource::Worktree(rel) => read_worktree(repo, rel),
        SideSource::Git(spec) => read_git_blob(repo, spec),
    }
}

fn worktree_meta(repo: &Path, rel: &str) -> Result<SideMeta, String> {
    let path = worktree_file(repo, rel)?;
    let meta = match std::fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(_) => {
            return Ok(SideMeta {
                missing: true,
                submodule: false,
                size: 0,
            })
        }
    };
    if meta.file_type().is_symlink() {
        let canon = dunce::canonicalize(&path).map_err(|err| format!("无法解析符号链接：{err}"))?;
        let root = dunce::canonicalize(repo).map_err(|err| format!("无法解析仓库路径：{err}"))?;
        if !canon.starts_with(&root) {
            return Err("文件位于仓库之外".into());
        }
    }
    if meta.is_dir() {
        return Ok(SideMeta {
            missing: false,
            submodule: true,
            size: 0,
        });
    }
    Ok(SideMeta {
        missing: false,
        submodule: false,
        size: meta.len(),
    })
}

fn read_worktree(repo: &Path, rel: &str) -> Result<Option<Vec<u8>>, String> {
    let path = worktree_file(repo, rel)?;
    if !path.exists() {
        return Ok(None);
    }
    let canon = dunce::canonicalize(&path).map_err(|err| format!("无法读取文件：{err}"))?;
    let root = dunce::canonicalize(repo).map_err(|err| format!("无法解析仓库路径：{err}"))?;
    if !canon.starts_with(&root) {
        return Err("文件位于仓库之外".into());
    }
    if !canon.is_file() {
        return Ok(None);
    }
    std::fs::read(&canon)
        .map(Some)
        .map_err(|err| format!("无法读取文件：{err}"))
}

fn git_meta(repo: &Path, spec: &str) -> Result<SideMeta, String> {
    let mut input = String::new();
    input.push_str(spec);
    input.push('\n');
    let output = git_output_stdin(
        git_command()
            .arg("-C")
            .arg(repo)
            .args(["cat-file", "--batch-check"]),
        input.as_bytes(),
    )?;
    if !output.status.success() {
        return Err(stderr_brief(&output));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().next().unwrap_or("");
    parse_batch_check(line)
}

fn parse_batch_check(line: &str) -> Result<SideMeta, String> {
    if line.is_empty() || line.ends_with(" missing") {
        return Ok(SideMeta {
            missing: true,
            submodule: false,
            size: 0,
        });
    }
    let mut parts = line.rsplitn(3, ' ');
    let size_text = parts.next().unwrap_or("0");
    let kind = parts.next().unwrap_or("");
    let size = size_text
        .parse::<u64>()
        .map_err(|_| format!("无法识别 Git 对象大小：{line}"))?;
    if kind == "commit" {
        return Ok(SideMeta {
            missing: false,
            submodule: true,
            size: 0,
        });
    }
    if kind != "blob" {
        return Err(format!("无法按文件读取：{kind}"));
    }
    Ok(SideMeta {
        missing: false,
        submodule: false,
        size,
    })
}

fn read_git_blob(repo: &Path, spec: &str) -> Result<Option<Vec<u8>>, String> {
    let mut input = String::new();
    input.push_str(spec);
    input.push('\n');
    let output = git_output_stdin(
        git_command()
            .arg("-C")
            .arg(repo)
            .args(["cat-file", "--batch"]),
        input.as_bytes(),
    )?;
    if !output.status.success() {
        return Err(stderr_brief(&output));
    }
    parse_batch_blob(&output.stdout)
}

fn parse_batch_blob(bytes: &[u8]) -> Result<Option<Vec<u8>>, String> {
    let Some(nl) = bytes.iter().position(|byte| *byte == b'\n') else {
        return Err("Git 输出不完整".into());
    };
    let header = String::from_utf8_lossy(&bytes[..nl]);
    if header.ends_with(" missing") {
        return Ok(None);
    }
    let mut parts = header.rsplitn(3, ' ');
    let size: usize = parts
        .next()
        .unwrap_or("0")
        .parse()
        .map_err(|_| "Git 输出不完整".to_string())?;
    let kind = parts.next().unwrap_or("");
    if kind != "blob" {
        return Ok(None);
    }
    let start = nl + 1;
    let end = start + size;
    if bytes.len() < end {
        return Err("Git 输出被截断".into());
    }
    Ok(Some(bytes[start..end].to_vec()))
}

fn ensure_repo(repo: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(repo);
    if !path.exists() {
        return Err("路径不存在".into());
    }
    if !path.is_dir() || !is_git_checkout(&path) {
        return Err("所选路径不是 Git 仓库".into());
    }
    Ok(path)
}

fn safe_rel_path(rel: &str) -> Result<(), String> {
    if rel.is_empty() || rel.contains('\0') || rel.contains('\n') || rel.contains('\r') {
        return Err("路径无效".into());
    }
    if rel.starts_with('/') || rel.starts_with('\\') {
        return Err("路径无效".into());
    }
    let native = Path::new(rel);
    if native.is_absolute() {
        return Err("路径无效".into());
    }
    for part in rel.split(['/', '\\']) {
        if part == ".." {
            return Err("路径无效".into());
        }
    }
    Ok(())
}

fn worktree_file(repo: &Path, rel: &str) -> Result<PathBuf, String> {
    safe_rel_path(rel)?;
    let mut path = repo.to_path_buf();
    for part in rel.split(['/', '\\']) {
        if part.is_empty() || part == "." {
            continue;
        }
        path.push(part);
    }
    Ok(path)
}

fn status_text(repo: &Path) -> Result<String, String> {
    // 详情里要逐个文件，所以未跟踪文件全部展开。汇总数字仍用默认的目录折叠。
    let output = git_output(
        git_command()
            .arg("-c")
            .arg("core.quotePath=true")
            .arg("--no-optional-locks")
            .arg("-C")
            .arg(repo)
            .args([
                "status",
                "--porcelain=v2",
                "--branch",
                "--untracked-files=all",
            ]),
    )?;
    if !output.status.success() {
        return Err(stderr_brief(&output));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn ensure_ref(repo: &Path, rev: &str) -> Result<(), String> {
    let output = git_output(git_command().arg("-C").arg(repo).args([
        "rev-parse",
        "--verify",
        "--quiet",
        &format!("{rev}^{{commit}}"),
    ]))?;
    if output.status.success() {
        Ok(())
    } else {
        Err("找不到上游引用。请先在列表里执行 Fetch。这里不会自动获取远端。".into())
    }
}

fn upstream_files(repo: &Path, upstream: &str) -> Result<Vec<FileChange>, String> {
    ensure_ref(repo, upstream)?;
    let output = git_output(git_command().arg("-C").arg(repo).args([
        "diff",
        "--name-status",
        "-z",
        "-M",
        upstream,
        "HEAD",
    ]))?;
    if !output.status.success() {
        return Err(stderr_brief(&output));
    }
    let mut files = parse_name_status_z(&output.stdout);
    sort_files(&mut files);
    Ok(files)
}

fn parse_status_files(text: &str, mode: CompareMode) -> Vec<FileChange> {
    let mut files = Vec::new();
    for raw in text.split('\n') {
        let line = raw.trim_end_matches('\r');
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(file) = parse_status_line(line, mode) {
            files.push(file);
        }
    }
    sort_files(&mut files);
    files
}

fn sort_files(files: &mut [FileChange]) {
    files.sort_by(|left, right| {
        area_rank(&left.area)
            .cmp(&area_rank(&right.area))
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.code.cmp(&right.code))
    });
}

fn area_rank(area: &str) -> u8 {
    match area {
        "conflict" => 0,
        "both" => 1,
        "staged" => 2,
        "unstaged" => 3,
        "untracked" => 4,
        "upstream" => 5,
        _ => 9,
    }
}

fn parse_status_line(line: &str, mode: CompareMode) -> Option<FileChange> {
    if let Some(rest) = line.strip_prefix("? ") {
        if !matches!(mode, CompareMode::WorktreeHead | CompareMode::WorktreeIndex) {
            return None;
        }
        return Some(FileChange {
            path: unquote_git_path(rest.trim()),
            old_path: None,
            area: "untracked".into(),
            code: "??".into(),
            label: "未跟踪".into(),
        });
    }
    if line.starts_with("1 ") {
        let (fields, path) = take_fields(line, 8)?;
        let xy = fields.get(1)?.as_str();
        let submodule = fields.iter().skip(3).take(3).any(|mode| mode == "160000");
        return xy_file(xy, unquote_git_path(path.trim()), None, submodule, mode);
    }
    if line.starts_with("2 ") {
        let (fields, rest) = take_fields(line, 9)?;
        let xy = fields.get(1)?.as_str();
        let submodule = fields.iter().skip(3).take(3).any(|mode| mode == "160000");
        let (path, old) = split_rename_paths(&rest)?;
        return xy_file(xy, path, Some(old), submodule, mode);
    }
    if line.starts_with("u ") {
        let (fields, path) = take_fields(line, 10)?;
        let xy = fields.get(1)?.as_str();
        return Some(FileChange {
            path: unquote_git_path(path.trim()),
            old_path: None,
            area: "conflict".into(),
            code: xy.to_string(),
            label: conflict_label(xy),
        });
    }
    None
}

fn xy_file(
    xy: &str,
    path: String,
    old_path: Option<String>,
    submodule: bool,
    mode: CompareMode,
) -> Option<FileChange> {
    let mut chars = xy.chars();
    let index = chars.next()?;
    let work = chars.next()?;
    let staged = index != '.' && index != ' ';
    let unstaged = work != '.' && work != ' ';
    let (area, code) = match mode {
        CompareMode::WorktreeHead => {
            if staged && unstaged {
                ("both", format!("{index}{work}"))
            } else if staged {
                ("staged", index.to_string())
            } else if unstaged {
                ("unstaged", work.to_string())
            } else {
                return None;
            }
        }
        CompareMode::WorktreeIndex => {
            if !unstaged {
                return None;
            }
            ("unstaged", work.to_string())
        }
        CompareMode::IndexHead => {
            if !staged {
                return None;
            }
            ("staged", index.to_string())
        }
        CompareMode::LocalUpstream => return None,
    };
    let label = if submodule {
        "子模块".to_string()
    } else {
        change_label(area, &code)
    };
    let code = if submodule { "S".to_string() } else { code };
    Some(FileChange {
        path,
        old_path,
        area: area.to_string(),
        code,
        label,
    })
}

fn split_rename_paths(rest: &str) -> Option<(String, String)> {
    let (path, old) = rest.trim().split_once('\t')?;
    Some((unquote_git_path(path.trim()), unquote_git_path(old.trim())))
}

fn take_fields(line: &str, count: usize) -> Option<(Vec<String>, String)> {
    let mut rest = line.trim_end_matches('\r');
    let mut fields = Vec::with_capacity(count);
    for _ in 0..count {
        rest = rest.trim_start();
        if rest.is_empty() {
            return None;
        }
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        fields.push(rest[..end].to_string());
        rest = &rest[end..];
    }
    Some((fields, rest.trim_start().to_string()))
}

/// 解开 `core.quotePath` 打开时的 C 风格引号。未加引号的路径原样返回。
fn unquote_git_path(raw: &str) -> String {
    let raw = raw.trim();
    if raw.len() < 2 || !raw.starts_with('"') || !raw.ends_with('"') {
        return raw.to_string();
    }
    let bytes = raw.as_bytes();
    let inner = &bytes[1..bytes.len() - 1];
    let mut out = Vec::with_capacity(inner.len());
    let mut index = 0;
    while index < inner.len() {
        if inner[index] == b'\\' && index + 1 < inner.len() {
            let next = inner[index + 1];
            match next {
                b'\\' => out.push(b'\\'),
                b'"' => out.push(b'"'),
                b'n' => out.push(b'\n'),
                b't' => out.push(b'\t'),
                b'r' => out.push(b'\r'),
                b'a' => out.push(0x07),
                b'b' => out.push(0x08),
                b'f' => out.push(0x0c),
                b'v' => out.push(0x0b),
                _ if next.is_ascii_digit() && next <= b'7' => {
                    let mut value = 0u32;
                    let mut used = 0;
                    while used < 3 && index + 1 + used < inner.len() {
                        let digit = inner[index + 1 + used];
                        if !digit.is_ascii_digit() || digit > b'7' {
                            break;
                        }
                        value = value * 8 + u32::from(digit - b'0');
                        used += 1;
                    }
                    out.push(value as u8);
                    index += 1 + used;
                    continue;
                }
                other => out.push(other),
            }
            index += 2;
        } else {
            out.push(inner[index]);
            index += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn parse_name_status_z(bytes: &[u8]) -> Vec<FileChange> {
    let text = String::from_utf8_lossy(bytes);
    let parts: Vec<&str> = text.split('\0').collect();
    let mut index = 0;
    let mut files = Vec::new();
    while index < parts.len() {
        let status = parts[index];
        if status.is_empty() {
            index += 1;
            continue;
        }
        let letter = status.chars().next().unwrap_or('?');
        if letter == 'R' || letter == 'C' {
            if index + 2 >= parts.len() {
                break;
            }
            let old = parts[index + 1].to_string();
            let path = parts[index + 2].to_string();
            if !path.is_empty() {
                files.push(FileChange {
                    path,
                    old_path: Some(old),
                    area: "upstream".into(),
                    code: letter.to_string(),
                    label: letter_label(letter),
                });
            }
            index += 3;
        } else {
            if index + 1 >= parts.len() {
                break;
            }
            let path = parts[index + 1].to_string();
            if !path.is_empty() {
                files.push(FileChange {
                    path,
                    old_path: None,
                    area: "upstream".into(),
                    code: letter.to_string(),
                    label: letter_label(letter),
                });
            }
            index += 2;
        }
    }
    files
}

fn change_label(area: &str, code: &str) -> String {
    match (area, code) {
        ("both", "MM") => "已暂存，工作区又有修改".into(),
        ("both", "AM") => "已暂存添加，工作区又有修改".into(),
        ("both", "AD") => "已暂存添加，工作区已删除".into(),
        ("both", "MD") => "已暂存修改，工作区已删除".into(),
        ("both", "RM") => "已重命名，工作区又有修改".into(),
        ("both", "RD") => "已重命名，工作区已删除".into(),
        ("both", _) => "暂存和工作区都有改动".into(),
        _ => letter_label(code.chars().next().unwrap_or('?')),
    }
}

fn letter_label(letter: char) -> String {
    match letter {
        'M' => "已修改".into(),
        'A' => "已添加".into(),
        'D' => "已删除".into(),
        'R' => "已重命名".into(),
        'C' => "已复制".into(),
        'T' => "类型变更".into(),
        'U' => "未合并".into(),
        _ => letter.to_string(),
    }
}

fn conflict_label(code: &str) -> String {
    match code {
        "UU" => "冲突：双方修改".into(),
        "AA" => "冲突：双方添加".into(),
        "DD" => "冲突：双方删除".into(),
        "AU" => "冲突：我方添加、对方修改".into(),
        "UA" => "冲突：对方添加、我方修改".into(),
        "DU" => "冲突：我方删除、对方修改".into(),
        "UD" => "冲突：对方删除、我方修改".into(),
        _ => format!("冲突：{code}"),
    }
}

pub fn count_lines(bytes: &[u8]) -> usize {
    if bytes.is_empty() {
        return 0;
    }
    let newlines = bytes.iter().filter(|byte| **byte == b'\n').count();
    if bytes.last() == Some(&b'\n') {
        newlines
    } else {
        newlines + 1
    }
}

pub fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8000).any(|byte| *byte == 0)
}

fn is_image_path(path: &str) -> bool {
    image_media_type(path).is_some()
}

fn image_media_type(path: &str) -> Option<&'static str> {
    let ext = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let ext = ext.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("");
    let ext = ext.to_ascii_lowercase();
    match ext.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "bmp" => Some("image/bmp"),
        "svg" => Some("image/svg+xml"),
        "ico" => Some("image/x-icon"),
        "tif" | "tiff" => Some("image/tiff"),
        "avif" => Some("image/avif"),
        _ => None,
    }
}

fn utf16_len(text: &str) -> u32 {
    text.encode_utf16().count() as u32
}

/// 按行切分，换行符算在上一行里。返回每个行首的 UTF-16 偏移，最后一个值是文本结尾。
fn line_offsets(text: &str) -> Vec<u32> {
    let mut offsets = vec![0];
    let mut acc = 0u32;
    let mut rest = text;
    while let Some(index) = rest.find('\n') {
        acc += utf16_len(&rest[..=index]);
        offsets.push(acc);
        rest = &rest[index + 1..];
    }
    if !rest.is_empty() {
        acc += utf16_len(rest);
        offsets.push(acc);
    }
    offsets
}

fn offset_at(offsets: &[u32], index: usize) -> u32 {
    offsets
        .get(index)
        .copied()
        .unwrap_or(*offsets.last().unwrap_or(&0))
}

fn scroll_line(index: usize, line_count: usize) -> u32 {
    if line_count == 0 {
        return 1;
    }
    let line = index + 1;
    if line > line_count {
        line_count as u32
    } else {
        line as u32
    }
}

/// 把左右文本分成连续的差异段。相同文本返回空列表。
pub fn diff_hunks(before: &str, after: &str) -> Vec<DiffHunk> {
    let diff = TextDiff::from_lines(before, after);
    let old_offsets = line_offsets(before);
    let new_offsets = line_offsets(after);
    let old_lines = old_offsets.len().saturating_sub(1);
    let new_lines = new_offsets.len().saturating_sub(1);
    let mut hunks = Vec::new();
    for op in diff.ops() {
        let (old_index, old_len, new_index, new_len) = match *op {
            DiffOp::Equal { .. } => continue,
            DiffOp::Delete {
                old_index,
                old_len,
                new_index,
            } => (old_index, old_len, new_index, 0),
            DiffOp::Insert {
                old_index,
                new_index,
                new_len,
            } => (old_index, 0, new_index, new_len),
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => (old_index, old_len, new_index, new_len),
        };
        let mut lines = Vec::new();
        for change in diff.iter_changes(op) {
            let kind = match change.tag() {
                ChangeTag::Delete => "delete",
                ChangeTag::Insert => "insert",
                ChangeTag::Equal => "same",
            };
            let mut text = change.value().to_string();
            if text.ends_with('\n') {
                text.pop();
                if text.ends_with('\r') {
                    text.pop();
                }
            }
            lines.push(DiffLine {
                kind: kind.to_string(),
                text,
            });
        }
        hunks.push(DiffHunk {
            from_a: offset_at(&old_offsets, old_index),
            to_a: offset_at(&old_offsets, old_index + old_len),
            from_b: offset_at(&new_offsets, new_index),
            to_b: offset_at(&new_offsets, new_index + new_len),
            old_line: scroll_line(old_index, old_lines.max(old_len)),
            new_line: scroll_line(new_index, new_lines.max(new_len)),
            lines,
        });
    }
    hunks
}

fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    let mut index = 0;
    while index + 3 <= data.len() {
        let value = (u32::from(data[index]) << 16)
            | (u32::from(data[index + 1]) << 8)
            | u32::from(data[index + 2]);
        out.push(TABLE[((value >> 18) & 63) as usize] as char);
        out.push(TABLE[((value >> 12) & 63) as usize] as char);
        out.push(TABLE[((value >> 6) & 63) as usize] as char);
        out.push(TABLE[(value & 63) as usize] as char);
        index += 3;
    }
    let rest = data.len() - index;
    if rest == 1 {
        let value = u32::from(data[index]) << 16;
        out.push(TABLE[((value >> 18) & 63) as usize] as char);
        out.push(TABLE[((value >> 12) & 63) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rest == 2 {
        let value = (u32::from(data[index]) << 16) | (u32::from(data[index + 1]) << 8);
        out.push(TABLE[((value >> 18) & 63) as usize] as char);
        out.push(TABLE[((value >> 12) & 63) as usize] as char);
        out.push(TABLE[((value >> 6) & 63) as usize] as char);
        out.push('=');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::{Command, Stdio};

    fn files_of(text: &str, mode: &str) -> Vec<FileChange> {
        parse_status_files(text, CompareMode::parse(mode).unwrap())
    }

    #[test]
    fn porcelain_file_list_keeps_paths_and_areas() {
        let text = "\
# branch.oid abcdef1234567890abcdef1234567890abcdef12
# branch.head main
# branch.upstream origin/main
# branch.ab +1 -2
1 .M N... 100644 100644 100644 a b src/main.rs
1 MM N... 100644 100644 100644 a b src/both.rs
1 M. N... 100644 100644 100644 a b src/staged.rs
u UU N... 100644 100644 100644 100644 a b c src/conflict.rs
? notes.txt
2 RM N... 100644 100644 100644 a b R100 new.txt\told.txt
";
        let head = files_of(text, "worktreeHead");
        assert_eq!(
            head.iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>(),
            vec![
                "src/conflict.rs",
                "new.txt",
                "src/both.rs",
                "src/staged.rs",
                "src/main.rs",
                "notes.txt",
            ]
        );
        let both = head.iter().find(|file| file.path == "src/both.rs").unwrap();
        assert_eq!(both.area, "both");
        assert_eq!(both.code, "MM");
        assert_eq!(both.label, "已暂存，工作区又有修改");
        let renamed = head.iter().find(|file| file.path == "new.txt").unwrap();
        assert_eq!(renamed.old_path.as_deref(), Some("old.txt"));
        assert_eq!(renamed.area, "both");
        assert_eq!(renamed.code, "RM");

        let index = files_of(text, "indexHead");
        assert!(index
            .iter()
            .any(|file| file.path == "src/staged.rs" && file.area == "staged"));
        assert!(index
            .iter()
            .all(|file| file.path != "src/main.rs" && file.path != "notes.txt"));
        assert!(index
            .iter()
            .any(|file| file.path == "src/conflict.rs" && file.area == "conflict"));

        let work = files_of(text, "worktreeIndex");
        assert!(work
            .iter()
            .any(|file| file.path == "src/main.rs" && file.code == "M"));
        assert!(work
            .iter()
            .any(|file| file.path == "notes.txt" && file.code == "??"));
        assert!(work.iter().all(|file| file.path != "src/staged.rs"));
    }

    #[test]
    fn porcelain_unquotes_spaces_and_octal_paths() {
        let text = "\
1 .M N... 100644 100644 100644 a b \"docs/my file.rs\"
1 A. N... 100644 100644 100644 a b \"\\344\\270\\255\\346\\226\\207.txt\"
2 R. N... 100644 100644 100644 a b R090 \"new file.txt\"\t\"old file.txt\"
? \"quote\\\"name.txt\"
";
        let files = files_of(text, "worktreeHead");
        let paths: Vec<_> = files.iter().map(|file| file.path.as_str()).collect();
        assert!(paths.contains(&"docs/my file.rs"));
        assert!(paths.contains(&"中文.txt"));
        assert!(paths.contains(&"new file.txt"));
        assert!(paths.contains(&"quote\"name.txt"));
        let renamed = files
            .iter()
            .find(|file| file.path == "new file.txt")
            .unwrap();
        assert_eq!(renamed.old_path.as_deref(), Some("old file.txt"));
        assert_eq!(renamed.area, "staged");
        assert_eq!(renamed.code, "R");
    }

    #[test]
    fn name_status_z_splits_renames_as_old_then_new() {
        let raw = b"M\0src/a.rs\0A\0src/b.rs\0D\0gone.rs\0R073\0old name.rs\0new name.rs\0";
        let files = parse_name_status_z(raw);
        assert_eq!(files.len(), 4);
        assert_eq!(files[0].code, "M");
        assert_eq!(files[0].path, "src/a.rs");
        assert_eq!(files[0].label, "已修改");
        let renamed = files.iter().find(|file| file.code == "R").unwrap();
        assert_eq!(renamed.old_path.as_deref(), Some("old name.rs"));
        assert_eq!(renamed.path, "new name.rs");
        assert_eq!(renamed.area, "upstream");
    }

    #[test]
    fn text_limit_rejects_only_when_over_the_line() {
        assert_eq!(text_limit_reason(TEXT_BYTE_LIMIT, 10), None);
        assert_eq!(
            text_limit_reason(TEXT_BYTE_LIMIT + 1, 10),
            Some("文件超过 1.5 MB，已跳过预览。")
        );
        assert_eq!(text_limit_reason(100, TEXT_LINE_LIMIT), None);
        assert_eq!(
            text_limit_reason(100, TEXT_LINE_LIMIT + 1),
            Some("文件超过 4 万行，已跳过预览。")
        );
        assert_eq!(count_lines(b""), 0);
        assert_eq!(count_lines(b"a\nb\n"), 2);
        assert_eq!(count_lines(b"a\nb"), 2);
        assert!(!looks_binary(b"hello"));
        assert!(looks_binary(b"hel\0lo"));
        assert_eq!(base64_encode(b"hi"), "aGk=");
    }

    #[test]
    fn diff_hunks_split_separate_edits_and_count_utf16() {
        assert!(diff_hunks("a\n", "a\n").is_empty());

        let inserted = diff_hunks("", "a\nb\n");
        assert_eq!(inserted.len(), 1);
        assert_eq!(inserted[0].from_a, inserted[0].to_a);
        assert_eq!(inserted[0].to_b - inserted[0].from_b, 4);
        assert_eq!(
            inserted[0]
                .lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b"]
        );

        let mut before = String::new();
        let mut after = String::new();
        for n in 1..=20 {
            before.push_str(&format!("l{n}\n"));
            let body = if n == 2 {
                "X"
            } else if n == 16 {
                "Y"
            } else {
                ""
            };
            if body.is_empty() {
                after.push_str(&format!("l{n}\n"));
            } else {
                after.push_str(body);
                after.push('\n');
            }
        }
        let hunks = diff_hunks(&before, &after);
        assert_eq!(hunks.len(), 2, "{hunks:?}");
        assert_eq!(hunks[0].new_line, 2);
        assert_eq!(hunks[1].new_line, 16);
        assert!(hunks[0]
            .lines
            .iter()
            .any(|line| line.kind == "delete" && line.text == "l2"));
        assert!(hunks[0]
            .lines
            .iter()
            .any(|line| line.kind == "insert" && line.text == "X"));

        let chinese = diff_hunks("甲\n乙\n", "甲\n丙\n");
        assert_eq!(chinese.len(), 1);
        assert_eq!(chinese[0].from_a, 2);
        assert_eq!(chinese[0].to_a, 4);
        assert_eq!(chinese[0].from_b, 2);
        assert_eq!(chinese[0].to_b, 4);
    }

    fn git_ok() -> bool {
        let mut command = Command::new("git");
        crate::gitinfo::hide_console_window(&mut command);
        command
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    fn git(dir: &Path, args: &[&str]) {
        let mut command = Command::new("git");
        crate::gitinfo::hide_console_window(&mut command);
        let status = command
            .current_dir(dir)
            .args(args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GCM_INTERACTIVE", "Never")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_IMPLICIT_WORK_TREE")
            .status()
            .expect("无法执行 git");
        assert!(status.success(), "git {args:?} 失败");
    }

    #[test]
    fn live_modes_and_preview_limits() {
        if !git_ok() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-b", "main"]);
        git(dir.path(), &["config", "user.email", "t@example.com"]);
        git(dir.path(), &["config", "user.name", "tester"]);
        git(dir.path(), &["config", "commit.gpgsign", "false"]);
        fs::write(dir.path().join("a.txt"), "one\ntwo\nthree\n").unwrap();
        git(dir.path(), &["add", "a.txt"]);
        git(dir.path(), &["commit", "-m", "init"]);

        fs::write(dir.path().join("a.txt"), "one\nTWO\nthree\n").unwrap();
        git(dir.path(), &["add", "a.txt"]);
        fs::write(dir.path().join("a.txt"), "one\nTWO\nthree\nfour\n").unwrap();
        fs::write(dir.path().join("new.txt"), "hello\n").unwrap();
        fs::write(dir.path().join("bin.dat"), b"abc\0def").unwrap();
        let png: &[u8] = &[
            0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00,
            0x00, 0x90, 0x77, 0x53, 0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x08,
            0xd7, 0x63, 0xf8, 0xcf, 0xc0, 0x00, 0x00, 0x00, 0x03, 0x00, 0x01, 0x00, 0x05, 0xfe,
            0x02, 0xfe, 0xdc, 0xcc, 0x59, 0xe7, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44,
            0xae, 0x42, 0x60, 0x82,
        ];
        fs::write(dir.path().join("pic.png"), png).unwrap();

        let repo = dir.path().to_string_lossy().into_owned();
        let head = list_changes(&repo, "worktreeHead").unwrap();
        let changed = head.files.iter().find(|file| file.path == "a.txt").unwrap();
        assert_eq!(changed.area, "both");
        assert!(head
            .files
            .iter()
            .any(|file| file.path == "new.txt" && file.area == "untracked"));

        let diff = load_file_diff(&repo, "worktreeHead", "a.txt", None).unwrap();
        assert_eq!(diff.kind, "text");
        assert!(diff.left_text.as_deref().unwrap().contains("two"));
        assert!(diff.right_text.as_deref().unwrap().contains("four"));
        assert!(!diff.hunks.is_empty());

        let index = list_changes(&repo, "indexHead").unwrap();
        assert!(index
            .files
            .iter()
            .any(|file| file.path == "a.txt" && file.area == "staged"));
        assert!(index.files.iter().all(|file| file.path != "new.txt"));
        let staged = load_file_diff(&repo, "indexHead", "a.txt", None).unwrap();
        assert!(staged.right_text.as_deref().unwrap().contains("TWO"));
        assert!(!staged.right_text.as_deref().unwrap().contains("four"));

        let work = list_changes(&repo, "worktreeIndex").unwrap();
        assert!(work
            .files
            .iter()
            .any(|file| file.path == "a.txt" && file.area == "unstaged"));
        assert!(work.files.iter().any(|file| file.path == "pic.png"));

        let image = load_file_diff(&repo, "worktreeHead", "pic.png", None).unwrap();
        assert_eq!(image.kind, "image");
        assert!(image.left_missing);
        assert_eq!(image.media_type.as_deref(), Some("image/png"));
        assert_eq!(
            image.right_image.as_deref(),
            Some(base64_encode(png).as_str())
        );
        assert!(image.left_text.is_none() && image.right_text.is_none());

        let binary = load_file_diff(&repo, "worktreeHead", "bin.dat", None).unwrap();
        assert_eq!(binary.kind, "binary");
        assert!(binary.left_text.is_none() && binary.right_text.is_none());
        assert!(binary.left_image.is_none() && binary.right_image.is_none());

        let mut big = vec![b'a'; (TEXT_BYTE_LIMIT as usize) + 8];
        big.push(b'\n');
        fs::write(dir.path().join("big.txt"), &big).unwrap();
        let large = load_file_diff(&repo, "worktreeHead", "big.txt", None).unwrap();
        assert_eq!(large.kind, "tooLarge");
        assert!(large.right_text.is_none());
        assert!(large.message.unwrap().contains("1.5 MB"));

        let mut long = String::new();
        for _ in 0..=TEXT_LINE_LIMIT {
            long.push_str("x\n");
        }
        fs::write(dir.path().join("long.txt"), long).unwrap();
        let long_diff = load_file_diff(&repo, "worktreeHead", "long.txt", None).unwrap();
        assert_eq!(long_diff.kind, "tooLarge");
        assert!(long_diff.message.unwrap().contains("4 万行"));
    }

    #[test]
    fn local_upstream_uses_existing_ref_and_does_not_fetch() {
        if !git_ok() {
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let origin = root.path().join("origin");
        fs::create_dir(&origin).unwrap();
        git(&origin, &["init", "-b", "main"]);
        git(&origin, &["config", "user.email", "t@example.com"]);
        git(&origin, &["config", "user.name", "tester"]);
        git(&origin, &["config", "commit.gpgsign", "false"]);
        fs::write(origin.join("a.txt"), "v1\n").unwrap();
        git(&origin, &["add", "a.txt"]);
        git(&origin, &["commit", "-m", "init"]);

        git(root.path(), &["clone", "origin", "local"]);
        let local = root.path().join("local");
        git(&local, &["config", "user.email", "t@example.com"]);
        git(&local, &["config", "user.name", "tester"]);
        git(&local, &["config", "commit.gpgsign", "false"]);

        fs::write(local.join("b.txt"), "local\n").unwrap();
        git(&local, &["add", "b.txt"]);
        git(&local, &["commit", "-m", "local"]);

        fs::write(origin.join("c.txt"), "remote-only\n").unwrap();
        git(&origin, &["add", "c.txt"]);
        git(&origin, &["commit", "-m", "remote"]);

        let before = {
            let mut command = Command::new("git");
            crate::gitinfo::hide_console_window(&mut command);
            command
                .current_dir(&local)
                .args(["rev-parse", "origin/main"])
                .output()
                .unwrap()
        };
        let repo = local.to_string_lossy().into_owned();
        let list = list_changes(&repo, "localUpstream").unwrap();
        assert!(list.can_compare_upstream);
        assert!(list
            .files
            .iter()
            .any(|file| file.path == "b.txt" && file.code == "A"));
        assert!(list.files.iter().all(|file| file.path != "c.txt"));
        let diff = load_file_diff(&repo, "localUpstream", "b.txt", None).unwrap();
        assert_eq!(diff.kind, "text");
        assert!(diff.left_missing);
        assert_eq!(diff.right_text.as_deref(), Some("local\n"));

        let after = {
            let mut command = Command::new("git");
            crate::gitinfo::hide_console_window(&mut command);
            command
                .current_dir(&local)
                .args(["rev-parse", "origin/main"])
                .output()
                .unwrap()
        };
        assert_eq!(
            before.stdout, after.stdout,
            "比较上游不应移动本地的远端引用"
        );
    }
}
