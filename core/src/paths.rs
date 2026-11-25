//! 路径规范化。配置里保存绝对路径，比较时忽略末尾分隔符；Windows 上忽略大小写。

use std::path::{Path, PathBuf};

/// 工作树根目录：`.git` 可以是目录，也可以是指向 gitdir 的文件（子模块、worktree）。
pub fn is_git_checkout(dir: &Path) -> bool {
    let git = dir.join(".git");
    let meta = match std::fs::symlink_metadata(&git) {
        Ok(meta) => meta,
        Err(_) => return false,
    };
    if meta.is_dir() {
        return true;
    }
    if meta.is_file() {
        if let Ok(text) = std::fs::read_to_string(&git) {
            return text.trim().to_ascii_lowercase().starts_with("gitdir:");
        }
    }
    false
}

/// `%APPDATA%\RepoUnion\config.json`，其他系统用各自的配置目录。
pub fn default_config_path() -> Result<PathBuf, String> {
    let base = dirs::config_dir().ok_or("无法定位用户配置目录")?;
    Ok(base.join("RepoUnion").join("config.json"))
}

pub fn path_key(path: &Path) -> String {
    let simplified = dunce::simplified(path);
    let text = simplified.to_string_lossy().replace('\\', "/");
    let trimmed = text.trim_end_matches('/');
    let key = if trimmed.is_empty() {
        text.as_str()
    } else {
        trimmed
    };
    #[cfg(windows)]
    {
        key.to_lowercase()
    }
    #[cfg(not(windows))]
    {
        key.to_string()
    }
}

/// 判断两个路径是否指向磁盘上的同一个目录。
///
/// 只比较字符串不够：Windows 上同一目录可能写成 8.3 短名（如 `RUNNER~1`）、带 `.` 段或大小写不同，
/// 而 Git 输出的总是长路径。这里先解析成真实路径再比较；路径不存在等无法解析时退回字符串比较。
pub fn same_dir(left: &Path, right: &Path) -> bool {
    match (dunce::canonicalize(left), dunce::canonicalize(right)) {
        (Ok(left), Ok(right)) => path_key(&left) == path_key(&right),
        _ => path_key(left) == path_key(right),
    }
}

pub fn repo_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| path.to_string())
}

pub fn normalize_dir(input: &str) -> Result<PathBuf, String> {
    let trimmed = input.trim().trim_matches('"');
    if trimmed.is_empty() {
        return Err("请填写路径".into());
    }
    let path = PathBuf::from(trimmed);
    if !path.exists() {
        return Err("路径不存在".into());
    }
    if !path.is_dir() {
        return Err("请选择文件夹".into());
    }
    dunce::canonicalize(&path).map_err(|err| format!("无法解析路径：{err}"))
}

/// 接受工作树根目录；如果用户选中了 `.git` 本身，则回到上一级。
pub fn normalize_repo(input: &str) -> Result<PathBuf, String> {
    let mut path = normalize_dir(input)?;
    if path.file_name().and_then(|name| name.to_str()) == Some(".git") {
        if let Some(parent) = path.parent() {
            path = dunce::canonicalize(parent).map_err(|err| format!("无法解析路径：{err}"))?;
        }
    }
    if !is_git_checkout(&path) {
        return Err("所选路径不是 Git 仓库（未找到 .git）".into());
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_key_drops_trailing_slash() {
        assert_eq!(
            path_key(Path::new("/tmp/repo/")),
            path_key(Path::new("/tmp/repo"))
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn path_key_is_case_sensitive_off_windows() {
        assert_ne!(
            path_key(Path::new("/tmp/Foo")),
            path_key(Path::new("/tmp/foo"))
        );
    }
}
