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
  failures: FetchFailure[];
  items: RepoStatus[];
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

export type FilterId = "all" | "dirty" | "unsynced" | "conflict" | "noUpstream";

export function repoName(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  const parts = trimmed.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

export function matchesFilter(item: RepoStatus, filter: FilterId): boolean {
  switch (filter) {
    case "all":
      return true;
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
