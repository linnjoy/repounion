import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  AppState,
  ChangeList,
  CompareMode,
  FetchReport,
  FileDiff,
  OpenResult,
  RescanResult,
  ScanReport,
  StatusReport,
  Workspace,
} from "./types";

export function getState(): Promise<AppState> {
  return invoke("get_state");
}

export function createWorkspace(name: string): Promise<Workspace> {
  return invoke("create_workspace", { name });
}

export function renameWorkspace(id: string, name: string): Promise<void> {
  return invoke("rename_workspace", { id, name });
}

export function deleteWorkspace(id: string): Promise<AppState> {
  return invoke("delete_workspace", { id });
}

export function openWorkspace(id: string): Promise<OpenResult> {
  return invoke("open_workspace", { id });
}

export function addStandaloneRepo(workspaceId: string, path: string): Promise<Workspace> {
  return invoke("add_standalone_repo", { workspaceId, path });
}

export function scanDirectory(workspaceId: string, path: string): Promise<ScanReport> {
  return invoke("scan_directory", { workspaceId, path });
}

export function commitDirectory(
  workspaceId: string,
  path: string,
  selected: string[],
  rememberAsSource: boolean,
): Promise<Workspace> {
  return invoke("commit_directory", { workspaceId, path, selected, rememberAsSource });
}

export function setSourceAutoScan(
  workspaceId: string,
  sourceId: string,
  autoScan: boolean,
): Promise<Workspace> {
  return invoke("set_source_auto_scan", { workspaceId, sourceId, autoScan });
}

export function rescanSource(workspaceId: string, sourceId: string): Promise<RescanResult> {
  return invoke("rescan_source", { workspaceId, sourceId });
}

export function removeRepo(workspaceId: string, path: string): Promise<Workspace> {
  return invoke("remove_repo", { workspaceId, path });
}

export function refreshStatus(workspaceId: string): Promise<StatusReport> {
  return invoke("refresh_status", { workspaceId });
}

export function fetchAll(workspaceId: string): Promise<FetchReport> {
  return invoke("fetch_all", { workspaceId });
}

export function listRepoChanges(path: string, mode: CompareMode): Promise<ChangeList> {
  return invoke("list_repo_changes", { path, mode });
}

export function loadFileDiff(
  path: string,
  mode: CompareMode,
  filePath: string,
  oldPath: string | null,
): Promise<FileDiff> {
  return invoke("load_file_diff", { path, mode, filePath, oldPath });
}

export function openInFolder(path: string): Promise<void> {
  return invoke("open_in_folder", { path });
}

export function openInVscode(path: string): Promise<void> {
  return invoke("open_in_vscode", { path });
}

export function openInTerminal(path: string): Promise<void> {
  return invoke("open_in_terminal", { path });
}

export async function pickDirectory(title: string): Promise<string | null> {
  const selected = await open({
    directory: true,
    multiple: false,
    title,
  });
  return typeof selected === "string" ? selected : null;
}
