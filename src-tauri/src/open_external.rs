//! 在系统文件管理器、VS Code 和终端中打开仓库目录。
//! 路径都作为参数或环境变量传递，不拼进 shell 命令字符串。

use std::path::Path;
use std::process::{Command, Stdio};

pub fn open_folder(path: &Path) -> Result<(), String> {
    ensure_dir(path)?;
    let mut command = folder_command(path);
    spawn(&mut command).map_err(|err| format!("无法打开目录：{err}"))
}

pub fn open_vscode(path: &Path) -> Result<(), String> {
    ensure_dir(path)?;
    let mut command = vscode_command(path);
    spawn(&mut command).map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            "未找到 VS Code。请安装并将 code 加入 PATH".to_string()
        } else {
            format!("无法打开 VS Code：{err}")
        }
    })
}

pub fn open_terminal(path: &Path) -> Result<(), String> {
    ensure_dir(path)?;
    if launch_terminal(path) {
        Ok(())
    } else {
        Err("无法打开终端".into())
    }
}

fn ensure_dir(path: &Path) -> Result<(), String> {
    if path.is_dir() {
        Ok(())
    } else {
        Err("目录不存在".into())
    }
}

fn spawn(command: &mut Command) -> std::io::Result<()> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

fn try_spawn(command: &mut Command) -> bool {
    spawn(command).is_ok()
}

#[cfg(target_os = "windows")]
fn folder_command(path: &Path) -> Command {
    let mut command = Command::new("explorer.exe");
    command.arg(path);
    command
}

#[cfg(target_os = "macos")]
fn folder_command(path: &Path) -> Command {
    let mut command = Command::new("open");
    command.arg(path);
    command
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn folder_command(path: &Path) -> Command {
    let mut command = Command::new("xdg-open");
    command.arg(path);
    command
}

#[cfg(target_os = "windows")]
fn vscode_command(path: &Path) -> Command {
    // code 在 Windows 上通常是 cmd 脚本。路径放进环境变量，避免拼接命令。
    let mut command = Command::new("powershell.exe");
    command
        .args(["-NoProfile", "-Command", "& code -- $env:REPOUNION_DIR"])
        .env("REPOUNION_DIR", path);
    command
}

#[cfg(not(target_os = "windows"))]
fn vscode_command(path: &Path) -> Command {
    let mut command = Command::new("code");
    command.arg(path);
    command
}

#[cfg(target_os = "windows")]
fn launch_terminal(path: &Path) -> bool {
    let mut windows_terminal = Command::new("wt.exe");
    windows_terminal.arg("-d").arg(path);
    if try_spawn(&mut windows_terminal) {
        return true;
    }
    let mut command = Command::new("powershell.exe");
    command
        .args([
            "-NoExit",
            "-NoProfile",
            "-Command",
            "Set-Location -LiteralPath $env:REPOUNION_DIR",
        ])
        .env("REPOUNION_DIR", path);
    try_spawn(&mut command)
}

#[cfg(target_os = "macos")]
fn launch_terminal(path: &Path) -> bool {
    let mut command = Command::new("open");
    command.args(["-a", "Terminal"]).arg(path);
    try_spawn(&mut command)
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn launch_terminal(path: &Path) -> bool {
    let path_text = path.to_string_lossy().to_string();
    let candidates: [(&str, Vec<String>); 4] = [
        (
            "gnome-terminal",
            vec!["--working-directory".into(), path_text.clone()],
        ),
        ("konsole", vec!["--workdir".into(), path_text.clone()]),
        (
            "xfce4-terminal",
            vec!["--working-directory".into(), path_text.clone()],
        ),
        (
            "x-terminal-emulator",
            vec!["--working-directory".into(), path_text],
        ),
    ];
    for (program, args) in candidates {
        let mut command = Command::new(program);
        command.args(args);
        if try_spawn(&mut command) {
            return true;
        }
    }
    false
}
