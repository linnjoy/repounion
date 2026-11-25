import { reactive } from "vue";
import * as api from "./api";
import type { DiffLayout, FetchFailure, FilterId, RepoStatus, Workspace } from "./types";

/** 与后端 `CANCELLED_MESSAGE` 保持一致，用来区分「被取消」和「真正失败」。 */
export const CANCELLED_MESSAGE = "已取消";

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
  /** 已请求取消 Fetch，等待后端收尾。 */
  cancelling: false,
  /** 本轮刷新或 Fetch 中还没出结果的仓库，键为仓库路径。 */
  pending: {} as Record<string, boolean>,
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

/** 窗口获得焦点触发自动刷新的最短间隔。避免来回切窗口时反复跑 git status。 */
const FOCUS_REFRESH_GAP_MS = 30_000;

function workspacePaths(workspace: Workspace | null): string[] {
  if (!workspace) {
    return [];
  }
  return workspace.sources.flatMap((source) => source.repos).concat(workspace.standaloneRepos);
}

function markAllPending(paths: string[]) {
  const pending: Record<string, boolean> = {};
  for (const path of paths) {
    pending[path] = true;
  }
  ui.pending = pending;
}

/** 后端每推送一个仓库就替换对应一行，其余行保持上一次的结果，不整表闪烁。 */
function upsertStatus(item: RepoStatus) {
  const index = ui.statuses.findIndex((status) => status.path === item.path);
  if (index >= 0) {
    ui.statuses[index] = item;
  } else {
    ui.statuses.push(item);
  }
  delete ui.pending[item.path];
}

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
  ui.cancelling = false;
  ui.pending = {};
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
  markAllPending(workspacePaths(activeWorkspace()));
  // 切换工作区或新一轮刷新开始后，旧批次迟到的推送一律丢弃。
  const isCurrent = () => ticket === generation && ui.activeWorkspaceId === id;
  try {
    const report = await api.refreshStatus(id, (item) => {
      if (isCurrent()) {
        upsertStatus(item);
      }
    });
    if (!isCurrent()) {
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
      ui.pending = {};
    }
  }
}

/** 窗口重新获得焦点时只读刷新一次本地状态（不联网），有节流。 */
export function refreshOnFocus() {
  if (ui.booting || ui.refreshing || ui.fetching || !ui.activeWorkspaceId) {
    return;
  }
  if (ui.refreshedAt && Date.now() - Date.parse(ui.refreshedAt) < FOCUS_REFRESH_GAP_MS) {
    return;
  }
  void refresh();
}

export async function fetchAll() {
  const id = ui.activeWorkspaceId;
  if (!id || ui.refreshing || ui.fetching) {
    return;
  }
  const ticket = ++generation;
  ui.fetching = true;
  ui.cancelling = false;
  ui.banner = null;
  ui.failures = [];
  markAllPending(workspacePaths(activeWorkspace()));
  const isCurrent = () => ticket === generation && ui.activeWorkspaceId === id;
  try {
    const report = await api.fetchAll(id, (progress) => {
      if (isCurrent()) {
        upsertStatus(progress.item);
      }
    });
    if (!isCurrent()) {
      return;
    }
    ui.statuses = report.items;
    ui.refreshedAt = new Date().toISOString();
    const failed = report.failures.filter((failure) => failure.message !== CANCELLED_MESSAGE);
    const skipped = report.failures.length - failed.length;
    // 失败清单只列真正的失败；被取消跳过的仓库只在横幅里计数，免得清单被刷屏。
    ui.failures = failed;
    if (report.gitMissing) {
      ui.banner = "未找到 Git，请先安装并加入 PATH";
    } else if (report.cancelled) {
      const parts = [`已取消 Fetch，${skipped} 个仓库未更新远端引用`];
      if (failed.length > 0) {
        parts.push(`${failed.length} 个仓库失败`);
      }
      ui.banner = `${parts.join("，")}；本地状态已重新读取`;
    } else if (failed.length > 0) {
      ui.banner = `${failed.length} 个仓库 Fetch 失败，本地状态已重新读取`;
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
      ui.cancelling = false;
      ui.pending = {};
    }
  }
}

export async function cancelFetch() {
  if (!ui.fetching || ui.cancelling) {
    return;
  }
  ui.cancelling = true;
  try {
    await api.cancelFetch();
  } catch (error) {
    ui.cancelling = false;
    ui.banner = messageOf(error);
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
