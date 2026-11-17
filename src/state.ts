import { reactive } from "vue";
import * as api from "./api";
import type { DiffLayout, FetchFailure, FilterId, RepoStatus, Workspace } from "./types";

export interface MenuState {
  path: string;
  x: number;
  y: number;
}

export interface NameDialogState {
  mode: "create" | "rename";
  id: string;
  name: string;
}

export const ui = reactive({
  booting: true,
  workspaces: [] as Workspace[],
  activeWorkspaceId: null as string | null,
  statuses: [] as RepoStatus[],
  refreshedAt: null as string | null,
  refreshing: false,
  fetching: false,
  banner: null as string | null,
  failures: [] as FetchFailure[],
  toast: null as string | null,
  filter: "all" as FilterId,
  query: "",
  folderDialog: false,
  nameDialog: null as NameDialogState | null,
  confirmDeleteId: null as string | null,
  confirmRemovePath: null as string | null,
  menu: null as MenuState | null,
  wsMenuId: null as string | null,
  detailPath: null as string | null,
  detailWidth: 720,
  diffLayout: "side" as DiffLayout,
});

let generation = 0;
let toastTimer = 0;

export function messageOf(error: unknown): string {
  if (typeof error === "string") {
    return error;
  }
  if (error instanceof Error && error.message) {
    return error.message;
  }
  return "操作失败";
}

export function toast(text: string) {
  ui.toast = text;
  window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => {
    if (ui.toast === text) {
      ui.toast = null;
    }
  }, 3600);
}

function replaceWorkspace(workspace: Workspace) {
  const index = ui.workspaces.findIndex((item) => item.id === workspace.id);
  if (index >= 0) {
    ui.workspaces[index] = workspace;
  } else {
    ui.workspaces.push(workspace);
  }
}

export function activeWorkspace(): Workspace | null {
  return ui.workspaces.find((item) => item.id === ui.activeWorkspaceId) ?? null;
}

export async function boot() {
  ui.booting = true;
  let pending: string | null = null;
  try {
    const state = await api.getState();
    ui.workspaces = state.workspaces;
    ui.activeWorkspaceId = state.activeWorkspaceId;
    if (ui.activeWorkspaceId && ui.workspaces.some((item) => item.id === ui.activeWorkspaceId)) {
      pending = ui.activeWorkspaceId;
    }
  } catch (error) {
    ui.banner = messageOf(error);
  } finally {
    ui.booting = false;
  }
  if (pending) {
    await selectWorkspace(pending);
  }
}

export async function selectWorkspace(id: string) {
  if (ui.activeWorkspaceId === id && (ui.statuses.length > 0 || ui.refreshedAt)) {
    return;
  }
  const ticket = ++generation;
  ui.activeWorkspaceId = id;
  ui.statuses = [];
  ui.refreshedAt = null;
  ui.failures = [];
  ui.banner = null;
  ui.detailPath = null;
  ui.menu = null;
  ui.wsMenuId = null;
  ui.fetching = false;
  ui.refreshing = false;
  try {
    const opened = await api.openWorkspace(id);
    if (ticket !== generation) {
      return;
    }
    replaceWorkspace(opened.workspace);
    if (opened.added > 0) {
      toast(`自动发现 ${opened.added} 个新仓库`);
    }
    await refresh();
  } catch (error) {
    if (ticket === generation) {
      ui.banner = messageOf(error);
    }
  }
}

export async function refresh() {
  const id = ui.activeWorkspaceId;
  if (!id || ui.fetching) {
    return;
  }
  const ticket = ++generation;
  ui.refreshing = true;
  try {
    const report = await api.refreshStatus(id);
    if (ticket !== generation || ui.activeWorkspaceId !== id) {
      return;
    }
    ui.statuses = report.items;
    ui.refreshedAt = new Date().toISOString();
    ui.failures = [];
    if (report.gitMissing) {
      ui.banner = "未找到 Git，请先安装并加入 PATH";
    }
  } catch (error) {
    if (ticket === generation) {
      ui.banner = messageOf(error);
    }
  } finally {
    if (ticket === generation) {
      ui.refreshing = false;
    }
  }
}

export async function fetchAll() {
  const id = ui.activeWorkspaceId;
  if (!id || ui.refreshing || ui.fetching) {
    return;
  }
  const ticket = ++generation;
  ui.fetching = true;
  ui.banner = null;
  try {
    const report = await api.fetchAll(id);
    if (ticket !== generation || ui.activeWorkspaceId !== id) {
      return;
    }
    ui.statuses = report.items;
    ui.refreshedAt = new Date().toISOString();
    ui.failures = report.failures;
    if (report.gitMissing) {
      ui.banner = "未找到 Git，请先安装并加入 PATH";
    } else if (report.failures.length > 0) {
      ui.banner = `${report.failures.length} 个仓库 Fetch 失败，本地状态已重新读取`;
    } else {
      toast("已刷新远端引用");
    }
  } catch (error) {
    if (ticket === generation) {
      ui.banner = messageOf(error);
    }
  } finally {
    if (ticket === generation) {
      ui.fetching = false;
    }
  }
}

export async function createWorkspace(name: string): Promise<string | null> {
  try {
    const workspace = await api.createWorkspace(name);
    replaceWorkspace(workspace);
    ui.nameDialog = null;
    await selectWorkspace(workspace.id);
    return null;
  } catch (error) {
    return messageOf(error);
  }
}

export async function renameWorkspace(id: string, name: string): Promise<string | null> {
  try {
    await api.renameWorkspace(id, name);
    const workspace = ui.workspaces.find((item) => item.id === id);
    if (workspace) {
      workspace.name = name.trim();
    }
    ui.nameDialog = null;
    return null;
  } catch (error) {
    return messageOf(error);
  }
}

export async function deleteWorkspace(id: string) {
  try {
    const state = await api.deleteWorkspace(id);
    ui.workspaces = state.workspaces;
    ui.confirmDeleteId = null;
    ui.wsMenuId = null;
    if (ui.activeWorkspaceId === id) {
      ui.statuses = [];
      ui.refreshedAt = null;
      ui.activeWorkspaceId = state.activeWorkspaceId;
      if (state.activeWorkspaceId) {
        await selectWorkspace(state.activeWorkspaceId);
      }
    }
  } catch (error) {
    ui.banner = messageOf(error);
  }
}

export async function addRepo() {
  const id = ui.activeWorkspaceId;
  if (!id) {
    return;
  }
  try {
    const picked = await api.pickDirectory("选择 Git 仓库");
    if (!picked) {
      return;
    }
    const workspace = await api.addStandaloneRepo(id, picked);
    replaceWorkspace(workspace);
    toast("已添加仓库");
    await refresh();
  } catch (error) {
    ui.banner = messageOf(error);
  }
}

export async function commitFolder(
  path: string,
  selected: string[],
  rememberAsSource: boolean,
): Promise<string | null> {
  const id = ui.activeWorkspaceId;
  if (!id) {
    return "请先选择工作区";
  }
  try {
    const workspace = await api.commitDirectory(id, path, selected, rememberAsSource);
    replaceWorkspace(workspace);
    ui.folderDialog = false;
    toast(rememberAsSource ? "已添加目录来源" : "已添加仓库");
    await refresh();
    return null;
  } catch (error) {
    return messageOf(error);
  }
}

export async function setAutoScan(sourceId: string, autoScan: boolean): Promise<string | null> {
  const id = ui.activeWorkspaceId;
  if (!id) {
    return "请先选择工作区";
  }
  try {
    const workspace = await api.setSourceAutoScan(id, sourceId, autoScan);
    replaceWorkspace(workspace);
    return null;
  } catch (error) {
    return messageOf(error);
  }
}

export async function rescan(sourceId: string) {
  const id = ui.activeWorkspaceId;
  if (!id) {
    return;
  }
  try {
    const result = await api.rescanSource(id, sourceId);
    replaceWorkspace(result.workspace);
    toast(result.added > 0 ? `新加入 ${result.added} 个仓库` : "没有发现新仓库");
    if (result.added > 0) {
      await refresh();
    }
  } catch (error) {
    ui.banner = messageOf(error);
  }
}

export async function removeRepo(path: string) {
  const id = ui.activeWorkspaceId;
  if (!id) {
    return;
  }
  try {
    const workspace = await api.removeRepo(id, path);
    replaceWorkspace(workspace);
    ui.statuses = ui.statuses.filter((item) => item.path !== path);
    if (ui.detailPath === path) {
      ui.detailPath = null;
    }
    ui.confirmRemovePath = null;
    ui.menu = null;
    toast("已从工作区移除");
  } catch (error) {
    ui.banner = messageOf(error);
  }
}

export async function openFolder(path: string) {
  ui.menu = null;
  try {
    await api.openInFolder(path);
  } catch (error) {
    ui.banner = messageOf(error);
  }
}

export async function openVscode(path: string) {
  ui.menu = null;
  try {
    await api.openInVscode(path);
  } catch (error) {
    ui.banner = messageOf(error);
  }
}

export async function openTerminal(path: string) {
  ui.menu = null;
  try {
    await api.openInTerminal(path);
  } catch (error) {
    ui.banner = messageOf(error);
  }
}
