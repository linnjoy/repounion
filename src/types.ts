export interface AppState {
  version: number;
  activeWorkspaceId: string | null;
  workspaces: Workspace[];
}

export interface Workspace {
  id: string;
  name: string;
  sources: SourceDirectory[];
  standaloneRepos: string[];
  /** 保护分支规则，`*` 匹配任意字符。 */
  protectedBranches: string[];
}

/** 本地分支的推送情况。 */
export interface BranchInfo {
  name: string;
  oid: string;
  isCurrent: boolean;
  upstream: string | null;
  /** 上游分支已在远端删除。 */
  upstreamGone: boolean;
  ahead: number;
  behind: number;
  /** 不在任何远端上的提交数；null 表示未统计（无远端或分支过多）。 */
  unpushed: number | null;
  lastCommitAt: number | null;
}

/** 当前仓库之外的其他工作区。 */
export interface WorktreeInfo {
  path: string;
  branch: string | null;
  head: string;
  /** 有改动的文件数；null 表示读取失败。 */
  changedFiles: number | null;
  prunable: boolean;
}

export interface SourceDirectory {
  id: string;
  path: string;
  autoScan: boolean;
  excluded: string[];
  repos: string[];
}

export type Tone = "dirty" | "unsynced" | "conflict" | "clean" | "noUpstream" | "error";

export interface RepoStatus {
  path: string;
  name: string;
  ok: boolean;
  error: string | null;
  branch: string;
  detached: boolean;
  upstream: string | null;
  hasUpstream: boolean;
  ahead: number;
  behind: number;
  diverged: boolean;
  changedFiles: number;
  conflictFiles: number;
  workspaceLabel: string;
  remoteLabel: string;
  tone: Tone;
  /** 已跟踪改动按类型拆分（均已包含在 changedFiles 里），重命名计入修改。 */
  modifiedFiles: number;
  addedFiles: number;
  deletedFiles: number;
  /** 未跟踪条目数（已包含在 changedFiles 里）。 */
  untrackedFiles: number;
  stashCount: number;
  branches: BranchInfo[];
  /** 除当前分支外有未推送提交的分支数。 */
  otherUnpushedBranches: number;
  hasRemote: boolean;
  worktrees: WorktreeInfo[];
  headOid: string;
  lastCommitAt: number | null;
  onProtectedBranch: boolean;
  /** 在保护分支上有改动或未推送提交。 */
  protectedRisk: boolean;
  /** 本地状态指纹，用于「标记已查看」。 */
  fingerprint: string;
  /** 上次标记已查看之后有新变化。 */
  changedSinceAck: boolean;
  /** 附加信息读取失败的说明。 */
  notes: string[];
}

export interface StatusReport {
  gitMissing: boolean;
  items: RepoStatus[];
}

export interface FetchFailure {
  path: string;
  name: string;
  message: string;
}

export interface FetchReport {
  gitMissing: boolean;
  /** 本次 Fetch 是否被用户取消；已完成仓库的结果仍然有效。 */
  cancelled: boolean;
  failures: FetchFailure[];
  items: RepoStatus[];
}

/** Fetch 过程中每完成一个仓库推送一次。 */
export interface FetchProgress {
  item: RepoStatus;
  /** fetch 失败、超时或被取消的原因；本地状态仍已重新读取。 */
  fetchError: string | null;
}

export interface ScanEntry {
  path: string;
  name: string;
  branch: string;
  tone: Tone;
  badge: string;
  alreadyInWorkspace: boolean;
}

export interface ScanReport {
  root: string;
  entries: ScanEntry[];
}

export interface OpenResult {
  workspace: Workspace;
  added: number;
}

export interface RescanResult {
  workspace: Workspace;
  added: number;
}

/** 筛选芯片（交互细则 1.2.2），互斥单选，与搜索取交集。 */
export type FilterId = "all" | "changed" | "dirty" | "unsynced" | "conflict" | "noUpstream";

export type CompareMode = "worktreeHead" | "worktreeIndex" | "indexHead" | "localUpstream";

export type DiffLayout = "side" | "inline";

export interface FileChange {
  path: string;
  oldPath: string | null;
  area: string;
  code: string;
  label: string;
}

export interface ChangeList {
  mode: CompareMode;
  upstream: string | null;
  ahead: number;
  behind: number;
  canCompareUpstream: boolean;
  files: FileChange[];
}

export interface HunkSpan {
  fromA: number;
  toA: number;
  fromB: number;
  toB: number;
  oldLine: number;
  newLine: number;
}

export type DiffKind = "text" | "image" | "binary" | "tooLarge" | "unsupported";

export interface FileDiff {
  path: string;
  oldPath: string | null;
  mode: CompareMode;
  kind: DiffKind;
  message: string | null;
  leftText: string | null;
  rightText: string | null;
  leftMissing: boolean;
  rightMissing: boolean;
  mediaType: string | null;
  leftImage: string | null;
  rightImage: string | null;
  hunks: HunkSpan[];
}

/* ==========================================================================
   状态判定（与设计规范《交互细则》1.2 的定义一一对应）
   ========================================================================== */

/** 有改动：已修改、未跟踪或冲突任一大于 0。 */
export function isDirty(item: RepoStatus): boolean {
  return item.ok && (item.changedFiles > 0 || item.conflictFiles > 0);
}

export function isDiverged(item: RepoStatus): boolean {
  return item.ok && item.diverged;
}

/** 未同步：当前分支超前或落后，或其他分支有未推送提交。 */
export function isUnsynced(item: RepoStatus): boolean {
  return item.ok && (item.ahead > 0 || item.behind > 0 || item.otherUnpushedBranches > 0);
}

/** 已跟踪改动数（修改 + 新增 + 删除），即列表里的「n 改」。 */
export function trackedChanges(item: RepoStatus): number {
  return item.modifiedFiles + item.addedFiles + item.deletedFiles;
}

export function matchesFilter(item: RepoStatus, filter: FilterId): boolean {
  switch (filter) {
    case "all":
      return true;
    case "changed":
      return item.changedSinceAck;
    case "dirty":
      return isDirty(item);
    case "unsynced":
      return isUnsynced(item);
    case "conflict":
      return item.ok && (item.conflictFiles > 0 || item.diverged);
    case "noUpstream":
      // 游离 HEAD 不算「无上游」，它在分支列单独标出。
      return item.ok && !item.detached && !item.hasUpstream;
  }
}

/**
 * 组内默认排序的紧急度（交互细则 1.2.1），数值越小越靠前：
 * 1 有新变化 · 2 冲突/分叉 · 3 保护分支风险 · 4 有改动 · 5 读取失败/超时 · 6 未推送 · 7 落后 · 8 干净。
 * `treatAsNew` 用于「反跳动」：刚标记已查看的仓库在下次刷新前仍按第 1 级排序。
 */
export function priorityTier(item: RepoStatus | undefined, treatAsNew = false, fetchFailed = false): number {
  if (!item) {
    return 9;
  }
  if (item.changedSinceAck || treatAsNew) {
    return 1;
  }
  if (item.ok && (item.conflictFiles > 0 || item.diverged)) {
    return 2;
  }
  if (item.protectedRisk) {
    return 3;
  }
  if (isDirty(item)) {
    return 4;
  }
  if (!item.ok || fetchFailed) {
    return 5;
  }
  if (item.ahead > 0 || item.otherUnpushedBranches > 0) {
    return 6;
  }
  if (item.behind > 0) {
    return 7;
  }
  return 8;
}

/** 左侧 3px 色条的语义（视觉规范 2.2）。返回 CSS 变量名与是否虚线。 */
export function barOf(item: RepoStatus | undefined, fetchFailed = false): { color: string; dashed: boolean } {
  if (!item) {
    return { color: "var(--status-neutral-bar)", dashed: false };
  }
  if (!item.ok || fetchFailed) {
    return { color: "var(--status-danger-bar)", dashed: true };
  }
  if (item.changedSinceAck) {
    return { color: "var(--status-new-bar)", dashed: false };
  }
  if (item.conflictFiles > 0 || item.diverged || item.protectedRisk) {
    return { color: "var(--status-danger-bar)", dashed: false };
  }
  if (isDirty(item)) {
    return { color: "var(--status-dirty-bar)", dashed: false };
  }
  if (isUnsynced(item)) {
    return { color: "var(--status-sync-bar)", dashed: false };
  }
  if (!item.hasUpstream || item.detached) {
    return { color: "var(--status-neutral-bar)", dashed: false };
  }
  return { color: "var(--status-clean-bar)", dashed: false };
}

/* ==========================================================================
   收工检查（布局规范 3），粒度到分支
   ========================================================================== */

export type WrapUpCategory = "protected" | "dirty" | "unpushed" | "worktree" | "stash";

export interface WrapUpItem {
  key: string;
  category: WrapUpCategory;
  repo: RepoStatus;
  /** 分支名、worktree 路径；stash 为 null。 */
  target: string | null;
  detail: string;
  /** 非当前分支。 */
  otherBranch?: boolean;
  /** 未推送条目的超前数与上游。 */
  ahead?: number;
  upstream?: string | null;
}

function changeSummary(item: RepoStatus): string {
  const parts: string[] = [];
  const tracked = trackedChanges(item);
  if (tracked > 0) {
    parts.push(`${tracked} 改`);
  }
  if (item.untrackedFiles > 0) {
    parts.push(`${item.untrackedFiles} 未跟踪`);
  }
  if (item.conflictFiles > 0) {
    parts.push(`${item.conflictFiles} 冲突`);
  }
  return parts.join(" · ");
}

export function wrapUpItems(item: RepoStatus): WrapUpItem[] {
  if (!item.ok) {
    return [];
  }
  const items: WrapUpItem[] = [];
  const current = item.branches.find((branch) => branch.isCurrent);
  if (item.protectedRisk) {
    const ahead = current?.unpushed ?? item.ahead;
    const detail = [changeSummary(item), ahead > 0 ? `超前 ${ahead}` : ""].filter(Boolean).join(" · ");
    items.push({ key: `protected:${item.path}`, category: "protected", repo: item, target: item.branch, detail });
  } else if (isDirty(item)) {
    items.push({ key: `dirty:${item.path}`, category: "dirty", repo: item, target: item.branch, detail: changeSummary(item) });
  }
  for (const branch of item.branches) {
    const unpushed = branch.unpushed ?? 0;
    // 当前分支已作为保护分支风险列出时不重复。
    if (unpushed <= 0 || (branch.isCurrent && item.protectedRisk)) {
      continue;
    }
    items.push({
      key: `unpushed:${item.path}:${branch.name}`,
      category: "unpushed",
      repo: item,
      target: branch.name,
      detail: branch.upstream && !branch.upstreamGone ? branch.upstream : "从未推送到任何远端",
      otherBranch: !branch.isCurrent,
      ahead: unpushed,
      upstream: branch.upstreamGone ? null : branch.upstream,
    });
  }
  for (const tree of item.worktrees) {
    if ((tree.changedFiles ?? 0) > 0) {
      items.push({
        key: `worktree:${item.path}:${tree.path}`,
        category: "worktree",
        repo: item,
        target: tree.branch ?? "游离 HEAD",
        detail: `${tree.changedFiles} 个文件未提交 · ${tree.path}`,
      });
    }
  }
  if (item.stashCount > 0) {
    items.push({
      key: `stash:${item.path}`,
      category: "stash",
      repo: item,
      target: null,
      detail: `${item.stashCount} 个 stash 未清理`,
    });
  }
  return items;
}

/** Unix 秒转成「3 分钟前」「2 天前」这样的相对时间。 */
export function relativeTime(seconds: number | null): string {
  if (!seconds) {
    return "—";
  }
  const minutes = Math.max(0, Math.floor((Date.now() / 1000 - seconds) / 60));
  if (minutes < 2) {
    return "刚刚";
  }
  if (minutes < 60) {
    return `${minutes} 分钟前`;
  }
  const hours = Math.floor(minutes / 60);
  if (hours < 24) {
    return `${hours} 小时前`;
  }
  const days = Math.floor(hours / 24);
  if (days < 30) {
    return `${days} 天前`;
  }
  const months = Math.floor(days / 30);
  return months < 12 ? `${months} 个月前` : `${Math.floor(months / 12)} 年前`;
}

export function repoName(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  const parts = trimmed.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}
