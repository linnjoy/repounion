//! 保存在用户目录里的工作区模型。
//!
//! 一个工作区包含若干「来源目录」和若干「单独添加的仓库」。
//! 来源目录可以自动扫描，并记住用户手动移除的路径，避免再加回来。

use serde::{Deserialize, Serialize};

/// 当前配置格式版本。
/// - 1：初版。
/// - 2：工作区增加 `protectedBranches`。读取 1 版时自动补默认值，保存时写为 2。
pub const CONFIG_VERSION: u32 = 2;

/// 缺少 version 字段的文件视为初版。
fn default_version() -> u32 {
    1
}

/// 新建工作区和从 1 版迁移时使用的默认保护分支。
pub fn default_protected_branches() -> Vec<String> {
    vec![
        "main".to_string(),
        "master".to_string(),
        "release/*".to_string(),
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppState {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub active_workspace_id: Option<String>,
    #[serde(default)]
    pub workspaces: Vec<Workspace>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            active_workspace_id: None,
            workspaces: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub sources: Vec<SourceDirectory>,
    #[serde(default)]
    pub standalone_repos: Vec<String>,
    /// 保护分支规则，`*` 匹配任意字符。在这些分支上有改动或未推送提交时警告。
    #[serde(default = "default_protected_branches")]
    pub protected_branches: Vec<String>,
}

impl Workspace {
    /// 当前纳入看板的全部仓库路径（不含排除清单）。
    pub fn repo_paths(&self) -> Vec<String> {
        let mut paths = Vec::new();
        for source in &self.sources {
            paths.extend(source.repos.iter().cloned());
        }
        paths.extend(self.standalone_repos.iter().cloned());
        paths
    }

    pub fn repo_count(&self) -> usize {
        self.sources
            .iter()
            .map(|source| source.repos.len())
            .sum::<usize>()
            + self.standalone_repos.len()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceDirectory {
    pub id: String,
    pub path: String,
    /// 打开工作区时是否自动发现新仓库。
    #[serde(default = "default_true")]
    pub auto_scan: bool,
    /// 用户从该来源移除的仓库。自动扫描不会再加回。
    #[serde(default)]
    pub excluded: Vec<String>,
    /// 当前从该来源纳入工作区的仓库。
    #[serde(default)]
    pub repos: Vec<String>,
}

fn default_true() -> bool {
    true
}
