<script setup lang="ts">
/**
 * 主列表单行（布局规范 2.1 / 2.3）。
 * 列：3px 色条 | 仓库名 + 新变化 + 弱化路径 | 分支 + 保护警告 | 工作区改动 | 远端同步 + 扩展哨兵 | 时间 | 操作。
 * 窄宽度下的列收缩由 styles.css 的 @container list 控制。
 */
import {
  Archive,
  ArrowDown,
  ArrowUp,
  Check,
  CloudOff,
  Ellipsis,
  FolderGit2,
  GitBranch,
  GitFork,
  GitMerge,
  Hourglass,
  LoaderCircle,
  ShieldAlert,
  Sparkles,
  TriangleAlert,
  Unlink2,
  X,
} from "lucide-vue-next";
import { computed } from "vue";
import { ackRepos, ui } from "../state";
import { barOf, relativeTime, repoName, trackedChanges, type FetchFailure, type RepoStatus } from "../types";

const props = defineProps<{ path: string; status: RepoStatus | undefined; failure: FetchFailure | undefined }>();
const emit = defineEmits<{ open: []; menu: [event: MouseEvent] }>();

const pending = computed(() => ui.pending[props.path] === true);
const refreshingRow = computed(() => pending.value && ui.refreshing);
const fetchingRow = computed(() => pending.value && ui.fetching);
const bar = computed(() => barOf(props.status, Boolean(props.failure)));
const timedOut = computed(() => props.failure?.message.includes("超时") ?? false);
const tracked = computed(() => (props.status ? trackedChanges(props.status) : 0));
const dirtyWorktrees = computed(() => props.status?.worktrees.filter((tree) => (tree.changedFiles ?? 0) > 0).length ?? 0);

const otherUnpushedTitle = computed(() =>
  (props.status?.branches ?? [])
    .filter((branch) => !branch.isCurrent && (branch.unpushed ?? 0) > 0)
    .map((branch) => `${branch.name}：${branch.unpushed} 个提交未推送`)
    .join("\n"),
);
const worktreeTitle = computed(() =>
  (props.status?.worktrees ?? [])
    .map((tree) => `${tree.branch ?? "游离 HEAD"}${tree.changedFiles ? `（${tree.changedFiles} 个文件未提交）` : ""}\n  ${tree.path}`)
    .join("\n"),
);

/** 「已刷新」微闪：动画结束后清掉标记，下次出结果再闪。 */
const flashing = computed(() => ui.flashed[props.path] !== undefined && !pending.value);
function endFlash() {
  delete ui.flashed[props.path];
}

const shortHead = computed(() => props.status?.headOid.slice(0, 7) ?? "");
</script>

<template>
  <div
    class="repo-row"
    :class="{
      selected: ui.detailPath === path,
      'is-new': status?.changedSinceAck,
      'is-refreshing': refreshingRow,
      cursor: ui.cursorPath === path && ui.detailPath !== path,
      'flash-done': flashing,
    }"
    :title="path"
    @click="emit('open')"
    @animationend="endFlash"
  >
    <div class="row-bar" :class="{ dashed: bar.dashed, 'pulse-bar': refreshingRow }" :style="{ background: bar.color }" />

    <div class="col-name">
      <div class="name-line">
        <span class="repo-title">{{ status?.name ?? repoName(path) }}</span>
        <span v-if="status?.changedSinceAck" class="badge badge-new" title="上次标记已查看之后，HEAD、本地分支、stash 或改动文件发生了变化">
          <Sparkles :size="12" :stroke-width="1.85" />新变化
        </span>
      </div>
      <div class="col-path">{{ path }}</div>
    </div>

    <div class="col-branch">
      <template v-if="status?.ok">
        <span v-if="status.detached" class="branch-pill detached" title="游离 HEAD：当前不在任何分支上">
          <Unlink2 :size="12" :stroke-width="1.85" /><span>游离 HEAD ({{ shortHead }})</span>
        </span>
        <span v-else-if="status.protectedRisk" class="branch-pill protected-warn" title="警告：正在保护分支上修改或提交">
          <ShieldAlert :size="12" :stroke-width="1.85" /><span>{{ status.branch }} · 保护</span>
        </span>
        <span v-else class="branch-pill" :title="status.upstream ? `${status.branch} → ${status.upstream}` : status.branch">
          <GitBranch :size="12" :stroke-width="1.85" /><span>{{ status.branch }}</span>
        </span>
      </template>
      <span v-else class="text-muted">—</span>
    </div>

    <div class="col-worktree-status">
      <span v-if="refreshingRow" class="badge badge-neutral">
        <LoaderCircle :size="12" class="spin" />刷新中…
      </span>
      <span v-else-if="!status" class="badge badge-neutral badge-dashed">尚未刷新</span>
      <span v-else-if="!status.ok" class="badge badge-danger badge-dashed" :title="status.error ?? ''">
        <X :size="12" :stroke-width="2" />读取失败
      </span>
      <span v-else-if="status.conflictFiles > 0" class="badge badge-danger" title="存在冲突文件">
        <TriangleAlert :size="12" :stroke-width="1.85" />{{ status.conflictFiles }} 冲突<template v-if="tracked"> · {{ tracked }} 改</template>
      </span>
      <span
        v-else-if="status.changedFiles > 0"
        class="badge badge-dirty"
        :title="`修改 ${status.modifiedFiles} · 新增 ${status.addedFiles} · 删除 ${status.deletedFiles} · 未跟踪 ${status.untrackedFiles}`"
      >
        ●
        <template v-if="tracked">{{ tracked }} 改</template>
        <template v-if="tracked && status.untrackedFiles"> · </template>
        <template v-if="status.untrackedFiles">{{ status.untrackedFiles }} 未跟踪</template>
      </span>
      <span v-else class="badge badge-clean"><Check :size="12" :stroke-width="2" />干净</span>
    </div>

    <div class="col-remote">
      <span v-if="fetchingRow" class="badge badge-sync"><LoaderCircle :size="12" class="spin" />Fetch 中…</span>
      <span v-else-if="status && !status.ok" class="error-text" :title="status.error ?? ''">{{ status.error }}</span>
      <span v-else-if="failure && timedOut" class="badge badge-danger badge-dashed" :title="failure.message">
        <Hourglass :size="12" :stroke-width="1.85" />Fetch 超时
      </span>
      <span v-else-if="failure" class="badge badge-danger badge-dashed" :title="failure.message">
        <X :size="12" :stroke-width="2" />Fetch 失败
      </span>
      <template v-else-if="status">
        <span v-if="status.detached" class="badge badge-neutral badge-dashed"><Unlink2 :size="12" :stroke-width="1.85" />游离 HEAD</span>
        <span
          v-else-if="!status.hasUpstream"
          class="badge badge-neutral badge-dashed"
          :title="status.hasRemote ? '当前分支没有上游' : '仓库没有配置任何远端'"
        >
          <CloudOff :size="12" :stroke-width="1.85" />{{ status.hasRemote ? "无上游" : "无远端" }}
        </span>
        <span v-else-if="status.diverged" class="badge badge-danger mono" :title="status.remoteLabel">
          <GitFork :size="12" :stroke-width="1.85" />分叉 ↑{{ status.ahead }} ↓{{ status.behind }}
        </span>
        <span v-else-if="status.ahead > 0" class="badge badge-sync mono" :title="status.upstream ?? ''">
          <ArrowUp :size="12" :stroke-width="1.85" />超前 {{ status.ahead }}
        </span>
        <span v-else-if="status.behind > 0" class="badge badge-sync badge-dashed mono" :title="status.upstream ?? ''">
          <ArrowDown :size="12" :stroke-width="1.85" />落后 {{ status.behind }}
        </span>
        <span v-else class="synced" :title="status.upstream ?? ''">✓ 已同步</span>

        <span v-if="status.otherUnpushedBranches > 0" class="badge badge-sync" :title="otherUnpushedTitle">
          <span class="long">另 {{ status.otherUnpushedBranches }} 分支未推</span>
          <span class="short"><GitMerge :size="12" :stroke-width="1.85" />+{{ status.otherUnpushedBranches }}</span>
        </span>
        <span v-if="status.stashCount > 0" class="badge badge-neutral mono" :title="`${status.stashCount} 条 stash`">
          <Archive :size="12" :stroke-width="1.85" /><span class="long">stash </span>{{ status.stashCount }}
        </span>
        <span
          v-if="status.worktrees.length > 0"
          class="badge mono"
          :class="dirtyWorktrees > 0 ? 'badge-dirty' : 'badge-neutral'"
          :title="worktreeTitle"
        >
          <FolderGit2 :size="12" :stroke-width="1.85" />
          <span class="long">wt {{ status.worktrees.length }}<template v-if="dirtyWorktrees">（{{ dirtyWorktrees }} 改）</template></span>
          <span class="short">{{ status.worktrees.length }}<template v-if="dirtyWorktrees">·{{ dirtyWorktrees }}</template></span>
        </span>
        <span v-if="status.notes.length" class="badge badge-neutral badge-dashed" :title="status.notes.join('\n')">信息不全</span>
      </template>
    </div>

    <div class="col-time mono">{{ relativeTime(status?.lastCommitAt ?? null) }}</div>

    <div class="col-actions" @click.stop>
      <button
        v-if="status?.changedSinceAck"
        type="button"
        class="btn btn-ghost btn-icon btn-mark-seen-inline"
        title="标记已查看"
        @click="ackRepos([path])"
      >
        <Check :size="14" :stroke-width="2" />
      </button>
      <button type="button" class="btn btn-ghost btn-icon more" title="更多操作" @click="emit('menu', $event)">
        <Ellipsis :size="14" :stroke-width="1.85" />
      </button>
    </div>
  </div>
</template>
