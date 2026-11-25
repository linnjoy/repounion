//! 工作区配置的读写，以及添加、排除、自动扫描时的归并规则。

use std::collections::HashSet;
use std::path::Path;

use crate::model::{
    default_protected_branches, AppState, SourceDirectory, Workspace, CONFIG_VERSION,
};
use crate::paths::path_key;

pub fn load_state(path: &Path) -> Result<AppState, String> {
    if !path.exists() {
        return Ok(AppState::default());
    }
    let text = std::fs::read_to_string(path).map_err(|err| format!("无法读取配置：{err}"))?;
    let mut state: AppState =
        serde_json::from_str(&text).map_err(|err| format!("配置文件已损坏：{err}"))?;
    // 更新版本写入的配置可能含有本版本不认识的字段，保存时会被丢弃，所以拒绝打开。
    if state.version > CONFIG_VERSION {
        return Err(format!(
            "配置文件版本为 {}，比当前程序支持的 {} 更新，请升级 RepoUnion",
            state.version, CONFIG_VERSION
        ));
    }
    // 旧版本缺少的字段已由 serde 默认值补齐，这里只需更新版本号，下次保存即完成迁移。
    state.version = CONFIG_VERSION;
    Ok(state)
}

pub fn save_state(path: &Path, state: &AppState) -> Result<(), String> {
    let json =
        serde_json::to_string_pretty(state).map_err(|err| format!("无法序列化配置：{err}"))?;
    write_text_atomic(path, &json)
}

/// 原子地写入文本文件：先完整落盘到临时文件，再改名覆盖。配置和快照都用它。
pub(crate) fn write_text_atomic(path: &Path, json: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| format!("无法创建配置目录：{err}"))?;
    }
    let tmp = path.with_extension("json.tmp");
    {
        use std::io::Write;
        let mut file = std::fs::File::create(&tmp).map_err(|err| format!("无法写入配置：{err}"))?;
        file.write_all(format!("{json}\n").as_bytes())
            .map_err(|err| format!("无法写入配置：{err}"))?;
        // 先落盘再改名，避免断电后留下空文件。
        file.sync_all()
            .map_err(|err| format!("无法写入配置：{err}"))?;
    }
    // 不能先删旧文件再改名：两步之间崩溃会丢掉整个配置。
    // std::fs::rename 在 Windows 上使用 MoveFileExW + MOVEFILE_REPLACE_EXISTING，可直接覆盖。
    std::fs::rename(&tmp, path).map_err(|err| format!("无法替换配置：{err}"))?;
    Ok(())
}

pub fn workspace_mut<'a>(state: &'a mut AppState, id: &str) -> Result<&'a mut Workspace, String> {
    state
        .workspaces
        .iter_mut()
        .find(|workspace| workspace.id == id)
        .ok_or_else(|| "工作区不存在".to_string())
}

pub fn create_workspace(state: &mut AppState, name: &str) -> Result<Workspace, String> {
    let name = clean_name(name)?;
    let workspace = Workspace {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        sources: Vec::new(),
        standalone_repos: Vec::new(),
        protected_branches: default_protected_branches(),
    };
    state.active_workspace_id = Some(workspace.id.clone());
    state.workspaces.push(workspace.clone());
    Ok(workspace)
}

pub fn rename_workspace(state: &mut AppState, id: &str, name: &str) -> Result<(), String> {
    let name = clean_name(name)?;
    let workspace = workspace_mut(state, id)?;
    workspace.name = name;
    Ok(())
}

pub fn delete_workspace(state: &mut AppState, id: &str) -> Result<(), String> {
    let before = state.workspaces.len();
    state.workspaces.retain(|workspace| workspace.id != id);
    if state.workspaces.len() == before {
        return Err("工作区不存在".into());
    }
    if state.active_workspace_id.as_deref() == Some(id) {
        state.active_workspace_id = state
            .workspaces
            .first()
            .map(|workspace| workspace.id.clone());
    }
    Ok(())
}

pub fn set_active(state: &mut AppState, id: Option<String>) -> Result<(), String> {
    if let Some(ref id) = id {
        if state.workspaces.iter().all(|workspace| workspace.id != *id) {
            return Err("工作区不存在".into());
        }
    }
    state.active_workspace_id = id;
    Ok(())
}

pub fn add_standalone(workspace: &mut Workspace, path: &str) -> Result<(), String> {
    if contains_repo(workspace, path) {
        return Err("该仓库已在工作区中".into());
    }
    workspace.standalone_repos.push(path.to_string());
    Ok(())
}

pub fn remove_repo(workspace: &mut Workspace, path: &str) -> Result<(), String> {
    let target = key(path);
    let mut removed = false;
    if let Some(index) = workspace
        .standalone_repos
        .iter()
        .position(|repo| key(repo) == target)
    {
        workspace.standalone_repos.remove(index);
        removed = true;
    }
    for source in &mut workspace.sources {
        if let Some(index) = source.repos.iter().position(|repo| key(repo) == target) {
            let stored = source.repos.remove(index);
            push_unique(&mut source.excluded, stored);
            removed = true;
        }
    }
    if removed {
        Ok(())
    } else {
        Err("工作区中没有这个仓库".into())
    }
}

pub fn set_auto_scan(
    workspace: &mut Workspace,
    source_id: &str,
    auto_scan: bool,
) -> Result<(), String> {
    let source = workspace
        .sources
        .iter_mut()
        .find(|source| source.id == source_id)
        .ok_or("来源目录不存在")?;
    source.auto_scan = auto_scan;
    Ok(())
}

/// 把扫描到、且不在排除清单里的新仓库加入来源。
pub fn absorb_discovered(
    workspace: &mut Workspace,
    source_id: &str,
    discovered: &[String],
) -> Result<usize, String> {
    let source_index = workspace
        .sources
        .iter()
        .position(|source| source.id == source_id)
        .ok_or("来源目录不存在")?;
    let excluded: HashSet<String> = workspace.sources[source_index]
        .excluded
        .iter()
        .map(|path| key(path))
        .collect();
    let mut existing = all_keys(workspace);
    let mut added = Vec::new();
    for repo in discovered {
        let repo_key = key(repo);
        if excluded.contains(&repo_key) || existing.contains(&repo_key) {
            continue;
        }
        existing.insert(repo_key);
        added.push(repo.clone());
    }
    let count = added.len();
    workspace.sources[source_index].repos.extend(added);
    Ok(count)
}

/// 确认「从目录添加」。
///
/// `remember` 为真时记下这个目录。未被勾选、且当时不在工作区里的仓库写入排除清单。
/// 已在工作区中的仓库保持原位，这个动作不会把它们移走。
pub fn commit_folder(
    workspace: &mut Workspace,
    root: &str,
    discovered: &[String],
    selected: &[String],
    remember: bool,
) -> Result<(), String> {
    let root_key = key(root);
    let discovered_keys: HashSet<String> = discovered.iter().map(|path| key(path)).collect();
    let selected: Vec<String> = selected
        .iter()
        .filter(|path| discovered_keys.contains(&key(path)))
        .cloned()
        .collect();
    let selected_keys: HashSet<String> = selected.iter().map(|path| key(path)).collect();

    if !remember {
        if let Some(index) = workspace
            .sources
            .iter()
            .position(|source| key(&source.path) == root_key)
        {
            let source = workspace.sources.remove(index);
            for repo in source.repos {
                push_unique(&mut workspace.standalone_repos, repo);
            }
        }
        for repo in selected {
            if !contains_repo(workspace, &repo) {
                workspace.standalone_repos.push(repo);
            }
        }
        return Ok(());
    }

    if workspace
        .sources
        .iter()
        .all(|source| key(&source.path) != root_key)
    {
        workspace.sources.push(SourceDirectory {
            id: uuid::Uuid::new_v4().to_string(),
            path: root.to_string(),
            auto_scan: true,
            excluded: Vec::new(),
            repos: Vec::new(),
        });
    }
    let source_index = workspace
        .sources
        .iter()
        .position(|source| key(&source.path) == root_key)
        .expect("来源目录刚写入");
    workspace.sources[source_index].auto_scan = true;

    for repo in &selected {
        let repo_key = key(repo);
        workspace.sources[source_index]
            .excluded
            .retain(|path| key(path) != repo_key);
        if is_standalone(workspace, repo) || in_other_source(workspace, source_index, repo) {
            continue;
        }
        push_unique(&mut workspace.sources[source_index].repos, repo.clone());
    }

    for repo in discovered {
        if selected_keys.contains(&key(repo)) || contains_repo(workspace, repo) {
            continue;
        }
        push_unique(&mut workspace.sources[source_index].excluded, repo.clone());
    }
    Ok(())
}

fn clean_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("请填写工作区名称".into());
    }
    if name.chars().count() > 80 {
        return Err("工作区名称过长".into());
    }
    Ok(name.to_string())
}

fn key(path: &str) -> String {
    path_key(Path::new(path))
}

fn push_unique(list: &mut Vec<String>, path: String) {
    let repo_key = key(&path);
    if list.iter().any(|item| key(item) == repo_key) {
        return;
    }
    list.push(path);
}

pub fn contains_repo(workspace: &Workspace, path: &str) -> bool {
    let repo_key = key(path);
    workspace
        .standalone_repos
        .iter()
        .any(|repo| key(repo) == repo_key)
        || workspace
            .sources
            .iter()
            .any(|source| source.repos.iter().any(|repo| key(repo) == repo_key))
}

fn is_standalone(workspace: &Workspace, path: &str) -> bool {
    let repo_key = key(path);
    workspace
        .standalone_repos
        .iter()
        .any(|repo| key(repo) == repo_key)
}

fn in_other_source(workspace: &Workspace, source_index: usize, path: &str) -> bool {
    let repo_key = key(path);
    workspace.sources.iter().enumerate().any(|(index, source)| {
        index != source_index && source.repos.iter().any(|repo| key(repo) == repo_key)
    })
}

/// 设置保护分支规则：去掉首尾空白、空项和重复项，保持用户给出的顺序。允许清空。
pub fn set_protected_branches(
    workspace: &mut Workspace,
    patterns: &[String],
) -> Result<(), String> {
    let mut cleaned: Vec<String> = Vec::new();
    for pattern in patterns {
        let pattern = pattern.trim();
        if pattern.is_empty() || cleaned.iter().any(|item| item == pattern) {
            continue;
        }
        // 分支名不能含空白；含空白的规则永远匹配不到，直接拒绝以免用户误以为生效。
        if pattern.chars().any(char::is_whitespace) {
            return Err(format!("保护分支规则「{pattern}」不能包含空格"));
        }
        cleaned.push(pattern.to_string());
    }
    workspace.protected_branches = cleaned;
    Ok(())
}

fn all_keys(workspace: &Workspace) -> HashSet<String> {
    workspace
        .repo_paths()
        .into_iter()
        .map(|path| key(&path))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace() -> Workspace {
        Workspace {
            id: "ws".into(),
            name: "测试".into(),
            sources: Vec::new(),
            standalone_repos: Vec::new(),
            protected_branches: default_protected_branches(),
        }
    }

    #[test]
    fn migrates_v1_config_with_default_protected_branches() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(
            &path,
            r#"{"version":1,"activeWorkspaceId":"ws","workspaces":[{"id":"ws","name":"旧","sources":[],"standaloneRepos":[]}]}"#,
        )
        .unwrap();
        let state = load_state(&path).unwrap();
        assert_eq!(state.version, CONFIG_VERSION);
        assert_eq!(
            state.workspaces[0].protected_branches,
            default_protected_branches()
        );

        save_state(&path, &state).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"version\": 2"), "{text}");
        assert!(text.contains("protectedBranches"), "{text}");
    }

    #[test]
    fn refuses_config_from_newer_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, r#"{"version":99,"workspaces":[]}"#).unwrap();
        assert!(load_state(&path).unwrap_err().contains("升级"));
    }

    #[test]
    fn cleans_protected_branch_patterns() {
        let mut ws = workspace();
        let input: Vec<String> = [" main ", "", "main", "release/*"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        set_protected_branches(&mut ws, &input).unwrap();
        assert_eq!(ws.protected_branches, vec!["main", "release/*"]);
        assert!(set_protected_branches(&mut ws, &["my branch".to_string()]).is_err());
        set_protected_branches(&mut ws, &[]).unwrap();
        assert!(ws.protected_branches.is_empty(), "允许清空");
    }

    #[test]
    fn folder_remember_excludes_unchecked_and_rescan_respects_it() {
        let mut ws = workspace();
        commit_folder(
            &mut ws,
            "/work/src",
            &["/work/src/a".into(), "/work/src/b".into()],
            &["/work/src/a".into()],
            true,
        )
        .unwrap();
        assert_eq!(ws.sources.len(), 1);
        assert!(ws.sources[0].auto_scan);
        assert_eq!(ws.sources[0].repos, vec!["/work/src/a".to_string()]);
        assert_eq!(ws.sources[0].excluded, vec!["/work/src/b".to_string()]);

        let source_id = ws.sources[0].id.clone();
        let added = absorb_discovered(
            &mut ws,
            &source_id,
            &[
                "/work/src/a".into(),
                "/work/src/b".into(),
                "/work/src/c".into(),
            ],
        )
        .unwrap();
        assert_eq!(added, 1);
        assert_eq!(
            ws.sources[0].repos,
            vec!["/work/src/a".to_string(), "/work/src/c".to_string()]
        );

        remove_repo(&mut ws, "/work/src/a").unwrap();
        assert_eq!(ws.sources[0].repos, vec!["/work/src/c".to_string()]);
        assert!(ws.sources[0]
            .excluded
            .iter()
            .any(|path| path == "/work/src/a"));
        let added = absorb_discovered(
            &mut ws,
            &source_id,
            &[
                "/work/src/a".into(),
                "/work/src/b".into(),
                "/work/src/c".into(),
            ],
        )
        .unwrap();
        assert_eq!(added, 0);

        // 再次在对话框里勾选 a，会从排除清单放回来。
        commit_folder(
            &mut ws,
            "/work/src",
            &[
                "/work/src/a".into(),
                "/work/src/b".into(),
                "/work/src/c".into(),
            ],
            &["/work/src/a".into()],
            true,
        )
        .unwrap();
        assert!(ws.sources[0].repos.iter().any(|path| path == "/work/src/a"));
        assert!(ws.sources[0]
            .excluded
            .iter()
            .all(|path| path != "/work/src/a"));
        assert!(ws.sources[0]
            .excluded
            .iter()
            .any(|path| path == "/work/src/b"));
    }

    #[test]
    fn without_remember_repos_stay_standalone() {
        let mut ws = workspace();
        commit_folder(
            &mut ws,
            "/work/src",
            &["/work/src/a".into(), "/work/src/b".into()],
            &["/work/src/a".into()],
            false,
        )
        .unwrap();
        assert!(ws.sources.is_empty());
        assert_eq!(ws.standalone_repos, vec!["/work/src/a".to_string()]);
        remove_repo(&mut ws, "/work/src/a").unwrap();
        assert!(ws.standalone_repos.is_empty());
        assert!(ws.sources.is_empty());
    }

    #[test]
    fn unchecking_remember_promotes_source_repos() {
        let mut ws = workspace();
        commit_folder(
            &mut ws,
            "/work/src",
            &["/work/src/a".into()],
            &["/work/src/a".into()],
            true,
        )
        .unwrap();
        commit_folder(
            &mut ws,
            "/work/src",
            &["/work/src/a".into(), "/work/src/b".into()],
            &["/work/src/b".into()],
            false,
        )
        .unwrap();
        assert!(ws.sources.is_empty());
        assert_eq!(
            ws.standalone_repos,
            vec!["/work/src/a".to_string(), "/work/src/b".to_string()]
        );
    }

    #[test]
    fn duplicate_standalone_is_rejected_and_config_roundtrips() {
        let mut state = AppState::default();
        let ws = create_workspace(&mut state, "  个人项目  ").unwrap();
        assert_eq!(ws.name, "个人项目");
        add_standalone(workspace_mut(&mut state, &ws.id).unwrap(), "/repo/one").unwrap();
        let err =
            add_standalone(workspace_mut(&mut state, &ws.id).unwrap(), "/repo/one").unwrap_err();
        assert_eq!(err, "该仓库已在工作区中");

        let file = tempfile::NamedTempFile::new().unwrap();
        save_state(file.path(), &state).unwrap();
        let loaded = load_state(file.path()).unwrap();
        assert_eq!(loaded, state);
    }

    #[test]
    fn rejects_blank_name_and_missing_workspace() {
        let mut state = AppState::default();
        assert!(create_workspace(&mut state, "   ").is_err());
        assert!(rename_workspace(&mut state, "nope", "x").is_err());
        assert!(delete_workspace(&mut state, "nope").is_err());
    }
}
