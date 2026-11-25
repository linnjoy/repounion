<script setup lang="ts">
import { computed, ref } from "vue";
import RepoDetail from "./RepoDetail.vue";
import {
  addRepo,
  cancelFetch,
  fetchAll,
  openFolder,
  openTerminal,
  openVscode,
  refresh,
  rescan,
  setAutoScan,
  ui,
} from "../state";
import { matchesFilter, repoName, summarize, type FilterId, type RepoStatus, type SourceDirectory, type Workspace } from "../types";

const props = defineProps<{ workspace: Workspace }>();

const filters: { id: FilterId; label: string }[] = [
  { id: "all", label: "全部" },
  { id: "dirty", label: "有改动" },
  { id: "unsynced", label: "未同步" },
  { id: "conflict", label: "冲突" },
  { id: "noUpstream", label: "无上游" },
];

const statusMap = computed(() => {
  const map = new Map<string, RepoStatus>();
  for (const item of ui.statuses) {
    map.set(item.path, item);
  }
  return map;
});

const ownedStatuses = computed(() => {
  const paths = new Set(props.workspace.sources.flatMap((source) => source.repos).concat(props.workspace.standaloneRepos));
  return ui.statuses.filter((item) => paths.has(item.path));
});

const repoTotal = computed(
  () =>
    props.workspace.sources.reduce((sum, source) => sum + source.repos.length, 0) +
    props.workspace.standaloneRepos.length,
);

const summary = computed(() => summarize(repoTotal.value, ownedStatuses.value));

function filterCount(id: FilterId): number {
  if (id === "all") {
    return repoTotal.value;
  }
  return ownedStatuses.value.filter((item) => matchesFilter(item, id)).length;
}

function byName(left: string, right: string): number {
  return repoName(left).localeCompare(repoName(right), "zh-CN");
}

function visible(path: string): boolean {
  const status = statusMap.value.get(path);
  if (ui.filter !== "all" && (!status || !matchesFilter(status, ui.filter))) {
    return false;
  }
  const query = ui.query.trim().toLowerCase();
  if (!query) {
    return true;
  }
  const name = status?.name ?? repoName(path);
  const branch = status?.branch ?? "";
  return `${name} ${branch} ${path}`.toLowerCase().includes(query);
}

const standaloneVisible = computed(() => props.workspace.standaloneRepos.filter(visible).sort(byName));
const showStandalone = computed(
  () => props.workspace.standaloneRepos.length > 0 && (ui.filter === "all" && !ui.query.trim() || standaloneVisible.value.length > 0),
);

function sourceVisible(source: SourceDirectory): string[] {
  return source.repos.filter(visible).sort(byName);
}

function showSource(source: SourceDirectory): boolean {
  if (ui.filter === "all" && !ui.query.trim()) {
    return true;
  }
  return sourceVisible(source).length > 0;
}

const anyVisible = computed(() => showStandalone.value || props.workspace.sources.some(showSource));

function stripeClass(status: RepoStatus | undefined): string {
  switch (status?.tone) {
    case "dirty":
      return "dirty";
    case "unsynced":
      return "sync";
    case "conflict":
      return "conflict";
    case "clean":
      return "ok";
    default:
      return "none";
  }
}

function workspaceClass(status: RepoStatus | undefined): string {
  if (!status || !status.ok) {
    return "none";
  }
  if (status.conflictFiles > 0) {
    return "conflict";
  }
  if (status.changedFiles > 0) {
    return "dirty";
  }
  return "ok";
}

function remoteClass(status: RepoStatus | undefined): string {
  if (!status || !status.ok || !status.hasUpstream) {
    return "none";
  }
  if (status.diverged) {
    return "danger";
  }
  if (status.ahead === 0 && status.behind === 0) {
    return "ok";
  }
  return "info";
}

function workspaceText(path: string): string {
  const status = statusMap.value.get(path);
  if (status) {
    return status.workspaceLabel;
  }
  return ui.pending[path] ? "读取中" : "尚未刷新";
}

function remoteText(path: string): string {
  return statusMap.value.get(path)?.remoteLabel ?? "—";
}

function branchText(path: string): string {
  return statusMap.value.get(path)?.branch ?? "—";
}

function clock(value: string | null): string {
  if (!value) {
    return "";
  }
  const date = new Date(value);
  const hh = String(date.getHours()).padStart(2, "0");
  const mm = String(date.getMinutes()).padStart(2, "0");
  return `${hh}:${mm}`;
}

function askRemove(path: string) {
  ui.menu = null;
  ui.confirmRemovePath = path;
}

function openMenu(event: MouseEvent, path: string) {
  const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
  const height = 168;
  let y = rect.bottom + 4;
  if (y + height > window.innerHeight) {
    y = Math.max(8, rect.top - height);
  }
  ui.wsMenuId = null;
  ui.menu = { path, x: Math.max(8, rect.right - 188), y };
}

async function onAutoScan(source: SourceDirectory, event: Event) {
  const input = event.target as HTMLInputElement;
  const error = await setAutoScan(source.id, input.checked);
  if (error) {
    input.checked = source.autoScan;
    ui.banner = error;
  }
}

const busy = computed(() => ui.refreshing || ui.fetching);
const pendingCount = computed(() => Object.keys(ui.pending).length);
const dragging = ref(false);

function openDetail(path: string) {
  ui.detailPath = path;
}

function startDrag(event: PointerEvent) {
  event.preventDefault();
  const startX = event.clientX;
  const startWidth = ui.detailWidth;
  dragging.value = true;
  const move = (ev: PointerEvent) => {
    const next = startWidth - (ev.clientX - startX);
    const max = Math.max(520, window.innerWidth - 620);
    ui.detailWidth = Math.min(max, Math.max(520, next));
  };
  const stop = () => {
    dragging.value = false;
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", stop);
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", stop);
}
</script>

<template>
  <section class="main">
    <header class="toolbar">
      <div class="toolbar-row">
        <div>
          <h1>{{ workspace.name }}</h1>
          <div class="summary">
            <span>{{ summary.total }} 个仓库</span>
            <span class="pill warn">{{ summary.dirty }} 有改动</span>
            <span class="pill info">{{ summary.unsynced }} 与远端不一致</span>
            <span class="pill danger">{{ summary.conflict }} 冲突</span>
            <span class="pill ok">{{ summary.clean }} 干净且已同步</span>
            <span v-if="ui.refreshedAt" class="stamp">状态于 {{ clock(ui.refreshedAt) }} 刷新</span>
            <span v-else-if="busy" class="stamp">正在读取状态…</span>
          </div>
        </div>
        <div class="actions">
          <button type="button" class="btn" :disabled="busy" @click="refresh">
            {{ ui.refreshing ? "正在刷新…" : "刷新状态" }}
          </button>
          <!-- Fetch 进行中时同一个按钮变成「取消」，避免再找第二个按钮。 -->
          <button v-if="ui.fetching" type="button" class="btn" :disabled="ui.cancelling" @click="cancelFetch">
            {{ ui.cancelling ? "正在取消…" : `取消 Fetch（剩 ${pendingCount}）` }}
          </button>
          <button v-else type="button" class="btn" :disabled="busy" @click="fetchAll">Fetch 全部</button>
          <button type="button" class="btn" @click="addRepo">添加仓库</button>
          <button type="button" class="btn primary" @click="ui.folderDialog = true">添加目录</button>
        </div>
      </div>
      <div class="filters">
        <button
          v-for="item in filters"
          :key="item.id"
          type="button"
          class="chip"
          :class="{ on: ui.filter === item.id }"
          @click="ui.filter = item.id"
        >
          {{ item.label }} {{ filterCount(item.id) }}
        </button>
        <label class="search">
          <input v-model="ui.query" type="search" placeholder="搜索仓库、分支或路径" />
        </label>
      </div>
    </header>

    <div v-if="ui.banner" class="banner">
      {{ ui.banner }}
      <ul v-if="ui.failures.length">
        <li v-for="failure in ui.failures" :key="failure.path">
          {{ failure.name }}：{{ failure.message }}
          <div class="fail-path">{{ failure.path }}</div>
        </li>
      </ul>
    </div>

    <div class="board-body">
    <div class="content">
      <p v-if="repoTotal === 0 && workspace.sources.length === 0" class="loading-line">
        这个工作区还是空的。可以添加单个仓库，或扫描一个目录。
      </p>

      <section v-for="source in workspace.sources" v-show="showSource(source)" :key="source.id" class="group">
        <div class="group-h">
          <span class="tag">来源目录</span>
          <span class="path" :title="source.path">{{ source.path }}</span>
          <span class="meta">{{ source.repos.length }} 个仓库</span>
          <label class="autoscan">
            <input
              type="checkbox"
              :checked="source.autoScan"
              @change="onAutoScan(source, $event)"
            />
            <span class="track" />
            <span>打开工作区时自动扫描新仓库</span>
          </label>
          <button type="button" class="linkish" @click="rescan(source.id)">重新扫描</button>
        </div>
        <div v-if="sourceVisible(source).length" class="list">
          <article
            v-for="path in sourceVisible(source)"
            :key="path"
            class="row"
            :class="{ active: ui.detailPath === path, pending: ui.pending[path] }"
            @click="openDetail(path)"
          >
            <span class="stripe" :class="stripeClass(statusMap.get(path))" />
            <div class="repo-name" :title="repoName(path)">{{ statusMap.get(path)?.name ?? repoName(path) }}</div>
            <div class="branch" :title="branchText(path)">{{ branchText(path) }}</div>
            <div class="status-text" :class="workspaceClass(statusMap.get(path))" :title="statusMap.get(path)?.error || workspaceText(path)">
              {{ workspaceText(path) }}
            </div>
            <div class="sync-text" :class="remoteClass(statusMap.get(path))">{{ remoteText(path) }}</div>
            <div class="repo-path" :title="path">{{ path }}</div>
            <button type="button" class="more" :class="{ on: ui.menu?.path === path }" title="操作" @click.stop="openMenu($event, path)">⋯</button>
          </article>
        </div>
      </section>

      <section v-if="showStandalone" class="group">
        <div class="group-h">
          <span class="tag">单独添加的仓库</span>
          <span class="meta">{{ workspace.standaloneRepos.length }} 个</span>
        </div>
        <div v-if="standaloneVisible.length" class="list">
          <article
            v-for="path in standaloneVisible"
            :key="path"
            class="row"
            :class="{ active: ui.detailPath === path, pending: ui.pending[path] }"
            @click="openDetail(path)"
          >
            <span class="stripe" :class="stripeClass(statusMap.get(path))" />
            <div class="repo-name" :title="repoName(path)">{{ statusMap.get(path)?.name ?? repoName(path) }}</div>
            <div class="branch" :title="branchText(path)">{{ branchText(path) }}</div>
            <div class="status-text" :class="workspaceClass(statusMap.get(path))" :title="statusMap.get(path)?.error || workspaceText(path)">
              {{ workspaceText(path) }}
            </div>
            <div class="sync-text" :class="remoteClass(statusMap.get(path))">{{ remoteText(path) }}</div>
            <div class="repo-path" :title="path">{{ path }}</div>
            <button type="button" class="more" :class="{ on: ui.menu?.path === path }" title="操作" @click.stop="openMenu($event, path)">⋯</button>
          </article>
        </div>
      </section>

      <p v-if="repoTotal > 0 && !anyVisible" class="loading-line">没有符合筛选的仓库</p>
    </div>
    <template v-if="ui.detailPath">
      <div class="splitter" :class="{ on: dragging }" title="拖动调整详情宽度" @pointerdown="startDrag" />
      <RepoDetail :path="ui.detailPath" :width="ui.detailWidth" />
    </template>
    </div>

    <div
      v-if="ui.menu && !ui.menu.path.startsWith('ws:')"
      class="menu"
      :style="{ left: `${ui.menu.x}px`, top: `${ui.menu.y}px` }"
    >
      <button type="button" @click="openFolder(ui.menu.path)">打开目录</button>
      <button type="button" @click="openVscode(ui.menu.path)">在 VS Code 中打开</button>
      <button type="button" @click="openTerminal(ui.menu.path)">在终端打开</button>
      <button type="button" class="danger" @click="askRemove(ui.menu.path)">从工作区移除</button>
    </div>
  </section>
</template>
