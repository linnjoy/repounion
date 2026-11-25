<script setup lang="ts">
/**
 * 主区（布局规范 1、2、6）：顶栏 → Fetch 汇总条 → 筛选栏 → 按来源目录分组的一级列表；
 * 右侧可拖宽的详情面板。快捷键见《交互细则》1.1。
 */
import {
  Check,
  ChevronDown,
  ChevronRight,
  Code,
  Folder,
  FolderGit2,
  FolderPlus,
  LoaderCircle,
  Plus,
  RefreshCw,
  Search,
  Settings,
  Square,
  Terminal,
  Trash2,
  TriangleAlert,
  X,
} from "lucide-vue-next";
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import RepoDetail from "./RepoDetail.vue";
import RepoRow from "./RepoRow.vue";
import WrapUpView from "./WrapUpView.vue";
import {
  ackAllChanged,
  ackRepos,
  addRepo,
  cancelFetch,
  DETAIL_MAX_WIDTH,
  DETAIL_MIN_WIDTH,
  dismissFetchBanner,
  fetchAll,
  openFolder,
  openTerminal,
  openVscode,
  refresh,
  setFilter,
  ui,
} from "../state";
import {
  isDirty,
  isUnsynced,
  matchesFilter,
  priorityTier,
  repoName,
  wrapUpItems,
  type FetchFailure,
  type FilterId,
  type RepoStatus,
  type Workspace,
} from "../types";

const props = defineProps<{ workspace: Workspace }>();

const filters: { id: FilterId; label: string }[] = [
  { id: "all", label: "全部" },
  { id: "changed", label: "✦ 有新变化" },
  { id: "dirty", label: "● 有改动" },
  { id: "unsynced", label: "⇅ 未同步" },
  { id: "conflict", label: "▲ 冲突/分叉" },
  { id: "noUpstream", label: "○ 无上游" },
];

const statusMap = computed(() => {
  const map = new Map<string, RepoStatus>();
  for (const item of ui.statuses) {
    map.set(item.path, item);
  }
  return map;
});

const failureMap = computed(() => {
  const map = new Map<string, FetchFailure>();
  for (const failure of ui.failures) {
    map.set(failure.path, failure);
  }
  return map;
});

const ownedPaths = computed(() =>
  props.workspace.sources.flatMap((source) => source.repos).concat(props.workspace.standaloneRepos),
);
const ownedStatuses = computed(() => {
  const paths = new Set(ownedPaths.value);
  return ui.statuses.filter((item) => paths.has(item.path));
});
const repoTotal = computed(() => ownedPaths.value.length);

const counts = computed(() => {
  const list = ownedStatuses.value;
  return {
    changed: list.filter((item) => item.changedSinceAck).length,
    dirty: list.filter(isDirty).length,
    unsynced: list.filter(isUnsynced).length,
    wrapUp: list.reduce((sum, item) => sum + wrapUpItems(item).length, 0),
  };
});

function filterCount(id: FilterId): number {
  if (id === "all") {
    return repoTotal.value;
  }
  return ownedStatuses.value.filter((item) => matchesFilter(item, id)).length;
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

/** 组内排序：8 级紧急度 → 最近提交在前 → 名称（交互细则 1.2.1）。 */
function compare(left: string, right: string): number {
  const a = statusMap.value.get(left);
  const b = statusMap.value.get(right);
  const tier =
    priorityTier(a, ui.ackedSinceSort[left], failureMap.value.has(left)) -
    priorityTier(b, ui.ackedSinceSort[right], failureMap.value.has(right));
  if (tier !== 0) {
    return tier;
  }
  const time = (b?.lastCommitAt ?? 0) - (a?.lastCommitAt ?? 0);
  return time !== 0 ? time : repoName(left).localeCompare(repoName(right), "zh-CN");
}

interface Group {
  id: string;
  title: string;
  mono: boolean;
  paths: string[];
}

const groups = computed<Group[]>(() => {
  const list: Group[] = props.workspace.sources.map((source) => ({
    id: source.id,
    title: source.path,
    mono: true,
    paths: source.repos.filter(visible).sort(compare),
  }));
  if (props.workspace.standaloneRepos.length > 0) {
    list.push({
      id: "standalone",
      title: "独立添加的仓库",
      mono: false,
      paths: props.workspace.standaloneRepos.filter(visible).sort(compare),
    });
  }
  // 空分组自动隐藏（交互细则 1.2.2）。
  return list.filter((group) => group.paths.length > 0);
});

/** 键盘导航顺序：跳过折叠分组。 */
const navOrder = computed(() => groups.value.flatMap((group) => (ui.collapsedGroups[group.id] ? [] : group.paths)));

function groupStats(group: Group) {
  const list = group.paths.map((path) => statusMap.value.get(path)).filter((item): item is RepoStatus => Boolean(item));
  return {
    changed: list.filter((item) => item.changedSinceAck).length,
    dirty: list.filter(isDirty).length,
    warn: list.filter((item) => !item.ok || item.conflictFiles > 0 || item.diverged || item.protectedRisk).length,
  };
}

function toggleGroup(id: string) {
  ui.collapsedGroups[id] = !ui.collapsedGroups[id];
}

function resetFilters() {
  setFilter("all");
  ui.query = "";
}

/* ---------- 顶栏：刷新状态提示（布局规范 6.1） ---------- */
const now = ref(Date.now());
let clock = 0;
onMounted(() => {
  clock = window.setInterval(() => (now.value = Date.now()), 5000);
});
onUnmounted(() => window.clearInterval(clock));

const pendingCount = computed(() => Object.keys(ui.pending).length);
const refreshPill = computed(() => {
  const done = repoTotal.value - pendingCount.value;
  if (ui.fetching) {
    return { busy: true, text: ui.cancelling ? "正在取消 Fetch…" : `Fetch 中 ${done} / ${repoTotal.value}…` };
  }
  if (ui.autoRefreshing) {
    return { busy: true, text: "自动刷新中…" };
  }
  if (ui.refreshing) {
    return { busy: true, text: `正在刷新 ${done} / ${repoTotal.value}…` };
  }
  if (!ui.refreshedAt) {
    return { busy: false, text: "尚未刷新" };
  }
  const seconds = Math.max(0, Math.round((now.value - Date.parse(ui.refreshedAt)) / 1000));
  if (seconds < 5) {
    return { busy: false, text: "刚刚刷新" };
  }
  if (seconds < 60) {
    return { busy: false, text: `${seconds} 秒前刷新` };
  }
  const minutes = Math.floor(seconds / 60);
  return { busy: false, text: minutes < 60 ? `${minutes} 分钟前刷新` : `${Math.floor(minutes / 60)} 小时前刷新` };
});

/* ---------- Fetch 汇总条（布局规范 6.2） ---------- */
const timeoutCount = computed(() => ui.failures.filter((failure) => failure.message.includes("超时")).length);
const fetchSummary = computed(() => {
  const failed = ui.failures.length - timeoutCount.value;
  const parts: string[] = [];
  if (failed > 0) {
    parts.push(`${failed} 个仓库失败`);
  }
  if (timeoutCount.value > 0) {
    parts.push(`${timeoutCount.value} 个超时`);
  }
  const head = parts.length ? `上次 Fetch 有 ${parts.join("，")}` : "";
  return [head, ui.fetchNote].filter(Boolean).join("；");
});

/* ---------- 菜单 ---------- */
function openRowMenu(event: MouseEvent, path: string) {
  const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
  const height = 190;
  let y = rect.bottom + 4;
  if (y + height > window.innerHeight) {
    y = Math.max(8, rect.top - height);
  }
  ui.menu = { path, x: Math.max(8, rect.right - 196), y };
}

function openAddMenu(event: MouseEvent) {
  const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
  ui.menu = ui.menu?.path === "add:" ? null : { path: "add:", x: Math.max(8, rect.right - 196), y: rect.bottom + 4 };
}

function menuAct(action: (path: string) => unknown) {
  const path = ui.menu?.path;
  ui.menu = null;
  if (path) {
    void action(path);
  }
}

function askRemove(path: string) {
  ui.menu = null;
  ui.confirmRemovePath = path;
}

/* ---------- 详情面板拖宽（480 ~ 1200px） ---------- */
const dragging = ref(false);
function startDrag(event: PointerEvent) {
  event.preventDefault();
  const startX = event.clientX;
  const startWidth = ui.detailWidth;
  dragging.value = true;
  const move = (ev: PointerEvent) => {
    const next = startWidth - (ev.clientX - startX);
    // 同时保证主列表至少留 420px，避免列表被挤没。
    const room = window.innerWidth - 216 - 420;
    ui.detailWidth = Math.round(Math.min(DETAIL_MAX_WIDTH, room, Math.max(DETAIL_MIN_WIDTH, next)));
  };
  const stop = () => {
    dragging.value = false;
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", stop);
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", stop);
}

/* ---------- 快捷键（交互细则 1.1：输入框聚焦时屏蔽，Esc / F5 由 App 统一处理） ---------- */
const searchInput = ref<HTMLInputElement | null>(null);
let navTimer = 0;

function typing(target: EventTarget | null): boolean {
  const element = target as HTMLElement | null;
  return Boolean(element && (element.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(element.tagName)));
}

function moveCursor(step: number) {
  const order = navOrder.value;
  if (order.length === 0) {
    return;
  }
  const current = ui.cursorPath ?? ui.detailPath;
  const index = current ? order.indexOf(current) : -1;
  const next = order[Math.min(order.length - 1, Math.max(0, index < 0 ? 0 : index + step))];
  ui.cursorPath = next;
  document.querySelector(`[data-path="${CSS.escape(next)}"]`)?.scrollIntoView({ block: "nearest" });
  // 详情面板已打开时同步切换，防抖 80ms，避免长按方向键连续读取。
  if (ui.detailPath) {
    window.clearTimeout(navTimer);
    navTimer = window.setTimeout(() => (ui.detailPath = next), 80);
  }
}

function onKeyDown(event: KeyboardEvent) {
  if (ui.settingsId || ui.folderDialog || ui.nameDialog || ui.confirmDeleteId || ui.confirmRemovePath) {
    return;
  }
  if (event.ctrlKey && event.shiftKey && event.key.toLowerCase() === "f") {
    event.preventDefault();
    void fetchAll();
    return;
  }
  if (event.altKey && event.key.toLowerCase() === "w") {
    event.preventDefault();
    ui.view = ui.view === "sentinel" ? "wrapup" : "sentinel";
    return;
  }
  if (typing(event.target)) {
    return;
  }
  if ((event.ctrlKey && event.key.toLowerCase() === "f") || event.key === "/") {
    event.preventDefault();
    ui.view = "sentinel";
    searchInput.value?.focus();
    searchInput.value?.select();
    return;
  }
  if (ui.view !== "sentinel" || event.ctrlKey || event.altKey || event.metaKey) {
    return;
  }
  // 详情面板打开时 ↑/↓ 交给面板切换文件；否则在列表里移动。
  if (ui.detailPath && document.activeElement?.closest(".detail-panel")) {
    return;
  }
  if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    event.preventDefault();
    moveCursor(event.key === "ArrowDown" ? 1 : -1);
  } else if (event.key === "Enter" && ui.cursorPath) {
    event.preventDefault();
    ui.detailPath = ui.cursorPath;
  }
}

onMounted(() => window.addEventListener("keydown", onKeyDown));
onUnmounted(() => window.removeEventListener("keydown", onKeyDown));

watch(
  () => ui.detailPath,
  (path) => {
    if (path) {
      ui.cursorPath = path;
    }
  },
);
</script>

<template>
  <main class="main-col">
    <header class="topbar">
      <div class="topbar-left">
        <h1 class="ws-title" :title="workspace.name">{{ workspace.name }}</h1>
        <div class="summary-stats mono">
          <span>{{ repoTotal }} 仓库</span>
          <span v-if="counts.changed" class="new">· {{ counts.changed }} 新变化</span>
          <span v-if="counts.dirty" class="dirty extra">· {{ counts.dirty }} 有改动</span>
          <span v-if="counts.unsynced" class="sync extra">· {{ counts.unsynced }} 未同步</span>
        </div>
        <span
          class="focus-refresh-pill"
          :title="ui.refreshedAt ? `上次刷新：${new Date(ui.refreshedAt).toLocaleString()}。窗口重新获得焦点时会自动刷新本地状态。` : '窗口重新获得焦点时会自动刷新本地状态'"
        >
          <LoaderCircle v-if="refreshPill.busy" :size="12" class="spin" />
          {{ refreshPill.text }}
        </span>
      </div>

      <div class="topbar-right">
        <div class="mode-seg" role="tablist" title="Alt+W 切换">
          <button type="button" :class="{ active: ui.view === 'sentinel' }" title="全部哨兵" @click="ui.view = 'sentinel'">
            <span class="label">全部</span>哨兵
          </button>
          <button type="button" :class="{ active: ui.view === 'wrapup' }" title="收工检查" @click="ui.view = 'wrapup'">
            <template v-if="counts.wrapUp > 0">收工<span class="label">检查</span> <span class="mono">{{ counts.wrapUp }}</span></template>
            <template v-else><span class="done"><Check :size="12" :stroke-width="2" /></span><span class="label">已全部</span>收工</template>
          </button>
        </div>
        <button type="button" class="btn" :disabled="ui.refreshing || ui.fetching" title="刷新状态：只读本地，不联网（F5）" @click="refresh()">
          <RefreshCw :size="14" :stroke-width="1.75" :class="{ spin: ui.refreshing }" />
          <span class="label">刷新状态</span>
        </button>
        <button v-if="ui.fetching" type="button" class="btn" :disabled="ui.cancelling" title="终止正在进行的 Fetch" @click="cancelFetch">
          <Square :size="13" :stroke-width="1.75" />
          <span class="label">{{ ui.cancelling ? "正在取消…" : "取消 Fetch" }}</span>
        </button>
        <button
          v-else
          type="button"
          class="btn"
          :disabled="ui.refreshing"
          title="Fetch 全部：只更新远端引用，不合并、不改工作区（Ctrl+Shift+F）"
          @click="fetchAll"
        >
          <FolderGit2 :size="14" :stroke-width="1.75" />
          <span class="label">Fetch 全部</span>
        </button>
        <button type="button" class="btn menu-trigger" title="添加仓库或目录" @click.stop="openAddMenu">
          <Plus :size="14" :stroke-width="1.75" />
          <span class="label">添加</span>
        </button>
        <button type="button" class="btn" title="工作区设置" @click="ui.settingsId = workspace.id">
          <Settings :size="14" :stroke-width="1.75" />
          <span class="label">设置</span>
        </button>
      </div>
    </header>

    <div v-if="ui.failures.length || ui.fetchNote" class="fetch-banner" :class="{ warn: ui.failures.length === 0 }">
      <div class="fetch-banner-bar">
        <div>
          <TriangleAlert :size="14" :stroke-width="1.85" />
          <strong>{{ fetchSummary }}</strong>
          <button v-if="ui.failures.length" type="button" class="btn btn-sm" @click="ui.fetchBannerOpen = !ui.fetchBannerOpen">
            {{ ui.fetchBannerOpen ? "收起详情 ▴" : "展开详情 ▾" }}
          </button>
        </div>
        <div>
          <button type="button" class="btn btn-sm" :disabled="ui.fetching || ui.refreshing" @click="fetchAll">重新 Fetch</button>
          <button type="button" class="btn btn-ghost btn-icon" title="隐藏提示" @click="dismissFetchBanner">
            <X :size="14" />
          </button>
        </div>
      </div>
      <div v-if="ui.fetchBannerOpen && ui.failures.length" class="fetch-banner-details">
        <div v-for="failure in ui.failures" :key="failure.path" class="fetch-err-row">
          <span>
            <strong class="mono">{{ failure.name }}</strong>
            — <span class="why">{{ failure.message.includes("超时") ? "超时" : "Fetch 失败" }}：{{ failure.message }}</span>
          </span>
          <button type="button" class="btn btn-sm" @click="openTerminal(failure.path)">在终端排查</button>
        </div>
      </div>
    </div>

    <div v-if="ui.banner" class="fetch-banner warn">
      <div class="fetch-banner-bar">
        <div><TriangleAlert :size="14" :stroke-width="1.85" /><span>{{ ui.banner }}</span></div>
        <button type="button" class="btn btn-ghost btn-icon" title="隐藏提示" @click="ui.banner = null"><X :size="14" /></button>
      </div>
    </div>

    <template v-if="ui.view === 'sentinel'">
      <div class="filterbar">
        <div class="filter-chips">
          <button
            v-for="item in filters"
            :key="item.id"
            type="button"
            class="fchip"
            :class="{ active: ui.filter === item.id, empty: filterCount(item.id) === 0 }"
            @click="setFilter(item.id)"
          >
            <span>{{ item.label }}</span>
            <span class="fchip-count">{{ filterCount(item.id) }}</span>
          </button>
        </div>
        <div class="filter-right">
          <button v-if="counts.changed > 0" type="button" class="btn btn-sm btn-new" @click="ackAllChanged">
            <Check :size="12" :stroke-width="2" />全部标为已查看 ({{ counts.changed }})
          </button>
          <div class="search-box">
            <Search class="lead" :size="13" :stroke-width="2" />
            <input ref="searchInput" v-model="ui.query" type="text" placeholder="筛选仓库、分支或路径…（/）" />
            <button v-if="ui.query" type="button" class="clear" title="清空" @click="ui.query = ''"><X :size="12" /></button>
          </div>
        </div>
      </div>

      <div class="list-viewport">
        <div v-if="repoTotal === 0" class="state-card muted">
          <FolderPlus :size="28" :stroke-width="1.5" />
          <h4>这个工作区还是空的</h4>
          <p>扫描一个目录批量加入其中的 Git 仓库，或单独添加一个仓库。</p>
          <div class="actions">
            <button type="button" class="btn btn-primary" @click="ui.folderDialog = true">扫描目录</button>
            <button type="button" class="btn" @click="addRepo">添加单个仓库</button>
          </div>
        </div>
        <div v-else-if="groups.length === 0" class="state-card muted">
          <Search :size="28" :stroke-width="1.5" />
          <h4>当前筛选条件下没有匹配的仓库</h4>
          <p>可以清空搜索词或切换回「全部」查看工作区内全部仓库。</p>
          <button type="button" class="btn" @click="resetFilters">重置筛选</button>
        </div>

        <template v-for="group in groups" :key="group.id">
          <div class="group-header" @click="toggleGroup(group.id)">
            <div class="group-header-left">
              <ChevronRight v-if="ui.collapsedGroups[group.id]" :size="14" :stroke-width="1.85" />
              <ChevronDown v-else :size="14" :stroke-width="1.85" />
              <Folder :size="13" :stroke-width="1.85" />
              <span :class="group.mono ? 'group-path' : ''" :title="group.title">{{ group.title }}</span>
              <span class="mono text-muted">({{ group.paths.length }})</span>
            </div>
            <div class="group-stats">
              <span v-if="groupStats(group).changed" class="new">{{ groupStats(group).changed }} 新变化</span>
              <span v-if="groupStats(group).dirty" class="dirty">{{ groupStats(group).dirty }} 有改动</span>
              <span v-if="groupStats(group).warn" class="danger">{{ groupStats(group).warn }} 警告</span>
            </div>
          </div>
          <template v-if="!ui.collapsedGroups[group.id]">
            <RepoRow
              v-for="path in group.paths"
              :key="path"
              :data-path="path"
              :path="path"
              :status="statusMap.get(path)"
              :failure="failureMap.get(path)"
              @open="ui.detailPath = path"
              @menu="openRowMenu($event, path)"
            />
          </template>
        </template>
      </div>
    </template>

    <WrapUpView v-else :statuses="ownedStatuses" />
  </main>

  <template v-if="ui.detailPath">
    <div class="detail-resizer" :class="{ dragging }" title="左右拖动调整详情面板宽度（480 ~ 1200px）" @pointerdown="startDrag" />
    <!-- 不加 key：切换仓库时复用同一个编辑器实例，只替换文档 -->
    <RepoDetail :path="ui.detailPath" :width="ui.detailWidth" />
  </template>

  <div
    v-if="ui.menu && ui.menu.path === 'add:'"
    class="row-menu-popover"
    :style="{ left: `${ui.menu.x}px`, top: `${ui.menu.y}px` }"
  >
    <button type="button" class="row-menu-item" @click="ui.menu = null; ui.folderDialog = true">
      <FolderPlus :size="14" :stroke-width="1.85" />扫描目录并添加
    </button>
    <button type="button" class="row-menu-item" @click="ui.menu = null; addRepo()">
      <Plus :size="14" :stroke-width="1.85" />添加单个仓库
    </button>
  </div>
  <div
    v-else-if="ui.menu && !ui.menu.path.startsWith('ws:')"
    class="row-menu-popover"
    :style="{ left: `${ui.menu.x}px`, top: `${ui.menu.y}px` }"
  >
    <button v-if="statusMap.get(ui.menu.path)?.changedSinceAck" type="button" class="row-menu-item" @click="menuAct((path) => ackRepos([path]))">
      <Check :size="14" :stroke-width="1.85" />标记已查看
    </button>
    <button type="button" class="row-menu-item" @click="menuAct(openVscode)"><Code :size="14" :stroke-width="1.85" />在 VS Code 中打开</button>
    <button type="button" class="row-menu-item" @click="menuAct(openTerminal)"><Terminal :size="14" :stroke-width="1.85" />在终端打开</button>
    <button type="button" class="row-menu-item" @click="menuAct(openFolder)"><Folder :size="14" :stroke-width="1.85" />在资源管理器中打开</button>
    <div class="row-menu-sep" />
    <button type="button" class="row-menu-item danger" @click="askRemove(ui.menu.path)"><Trash2 :size="14" :stroke-width="1.85" />从工作区移除</button>
  </div>
</template>
