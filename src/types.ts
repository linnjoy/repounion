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

export type FilterId = "all" | "changed" | "wrapUp" | "dirty" | "unsynced" | "conflict" | "noUpstream";

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

export function repoName(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  const parts = trimmed.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

/** 「收工检查」里的一条待办。一个仓库可能有多条，粒度到分支。 */
export interface WrapUpItem {
  kind: "uncommitted" | "unpushed" | "stash" | "protected" | "worktree";
  /** 涉及的分支或 worktree 路径；仓库级的条目为 null。 */
  target: string | null;
  text: string;
}

/**
 * 列出一个仓库还没收尾的事项。规则与产品目标的痛点 a / c 对应：
 * 未提交改动、任何本地分支上的未推送提交、stash、保护分支风险、有改动的其他 worktree。
 */
export function wrapUpItems(item: RepoStatus): WrapUpItem[] {
  if (!item.ok) {
    return [];
  }
  const items: WrapUpItem[] = [];
  if (item.protectedRisk) {
    items.push({ kind: "protected", target: item.branch, text: `在保护分支 ${item.branch} 上有改动或未推送提交` });
  }
  if (item.changedFiles > 0 || item.conflictFiles > 0) {
    const untracked = item.untrackedFiles > 0 ? `（其中未跟踪 ${item.untrackedFiles}）` : "";
    items.push({ kind: "uncommitted", target: item.branch, text: `${item.changedFiles + item.conflictFiles} 个文件未提交${untracked}` });
  }
  for (const branch of item.branches) {
    if ((branch.unpushed ?? 0) > 0) {
      const how = branch.upstream && !branch.upstreamGone ? `超前 ${branch.upstream}` : "从未推送到任何远端";
      items.push({ kind: "unpushed", target: branch.name, text: `${branch.unpushed} 个提交未推送（${how}）` });
    }
  }
  if (item.stashCount > 0) {
    items.push({ kind: "stash", target: null, text: `${item.stashCount} 个 stash` });
  }
  for (const tree of item.worktrees) {
    if ((tree.changedFiles ?? 0) > 0) {
      items.push({ kind: "worktree", target: tree.path, text: `worktree ${tree.branch ?? "游离"} 有 ${tree.changedFiles} 个文件未提交` });
    }
  }
  return items;
}

export function matchesFilter(item: RepoStatus, filter: FilterId): boolean {
  switch (filter) {
    case "all":
      return true;
    case "changed":
      return item.changedSinceAck;
    case "wrapUp":
      return wrapUpItems(item).length > 0;
    case "dirty":
      return item.tone === "dirty";
    case "unsynced":
      return item.tone === "unsynced";
    case "conflict":
      return item.tone === "conflict";
    case "noUpstream":
      return item.ok && !item.hasUpstream;
  }
}

export interface Summary {
  total: number;
  dirty: number;
  unsynced: number;
  conflict: number;
  clean: number;
  noUpstream: number;
}

/** 四项汇总互斥，优先级与色条一致。无上游是可以重叠的筛选。 */
export function summarize(total: number, items: RepoStatus[]): Summary {
  const summary: Summary = {
    total,
    dirty: 0,
    unsynced: 0,
    conflict: 0,
    clean: 0,
    noUpstream: 0,
  };
  for (const item of items) {
    if (item.ok && !item.hasUpstream) {
      summary.noUpstream += 1;
    }
    switch (item.tone) {
      case "conflict":
        summary.conflict += 1;
        break;
      case "dirty":
        summary.dirty += 1;
        break;
      case "unsynced":
        summary.unsynced += 1;
        break;
      case "clean":
        summary.clean += 1;
        break;
      default:
        break;
    }
  }
  return summary;
}
