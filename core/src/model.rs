//! 保存在用户目录里的工作区模型。
//!
//! 一个工作区包含若干「来源目录」和若干「单独添加的仓库」。
//! 来源目录可以自动扫描，并记住用户手动移除的路径，避免再加回来。

use serde::{Deserialize, Serialize};

fn default_version() -> u32 {
    1
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
            version: 1,
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
