<script setup lang="ts">
/**
 * 详情与 Diff 审阅台（布局规范 5）。
 * - 宽度 ≥ 740px 左右分栏；480~739px 上下堆叠，并默认切到内联 diff。
 * - 比较方向用「左 → 右」明确标注；快捷键 1~4 切换比较方式、[ / ] 跳转差异、↑/↓ 切换文件。
 * - 文本复用同一个编辑器，图片只加载当前一对。
 */
import {
  ArrowDown,
  ArrowUp,
  Check,
  ChevronDown,
  ChevronUp,
  CircleCheck,
  Code,
  FileCode,
  Folder,
  RefreshCw,
  Terminal,
  TriangleAlert,
  X,
} from "lucide-vue-next";
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import * as api from "../api";
import { ackRepos, messageOf, openFolder, openTerminal, openVscode, ui } from "../state";
import type { CompareMode, FileChange, FileDiff } from "../types";
import { repoName } from "../types";
import DiffEditor from "./DiffEditor.vue";
import ImageCompare from "./ImageCompare.vue";

const props = defineProps<{ path: string; width: number }>();

/** 宽度小于 740px 时进入上下堆叠的窄模式（布局规范 5.1）。 */
const NARROW_WIDTH = 740;
const narrow = computed(() => props.width < NARROW_WIDTH);
const fileListCollapsed = ref(false);

const files = ref<FileChange[]>([]);
const selected = ref<FileChange | null>(null);
const diff = ref<FileDiff | null>(null);
const listError = ref<string | null>(null);
const diffError = ref<string | null>(null);
const listLoading = ref(false);
const diffLoading = ref(false);
const chunkTotal = ref(0);
const chunkIndex = ref(0);
const hunkFlash = ref(false);
const editor = ref<{ scrollTo: (index: number) => void } | null>(null);
const textDiff = computed(() => (diff.value?.kind === "text" ? diff.value : null));
const imageDiff = computed(() => (diff.value?.kind === "image" ? diff.value : null));

let listTicket = 0;
let diffTicket = 0;

const status = computed(() => ui.statuses.find((item) => item.path === props.path));
const canUpstream = computed(
  () => !!status.value?.hasUpstream && ((status.value?.ahead ?? 0) > 0 || (status.value?.behind ?? 0) > 0),
);

const modes = computed(() => {
  const items: { id: CompareMode; label: string; key: string }[] = [
    { id: "worktreeHead", label: "工作区 vs HEAD", key: "1" },
    { id: "worktreeIndex", label: "工作区 vs 暂存", key: "2" },
    { id: "indexHead", label: "暂存 vs HEAD", key: "3" },
  ];
  if (canUpstream.value) {
    items.push({ id: "localUpstream", label: "本地 vs 上游", key: "4" });
  }
  return items;
});

/** 比较方向说明（布局规范 5.2）。 */
const direction = computed(() => {
  const head = status.value?.headOid ? `HEAD (${status.value.headOid.slice(0, 7)})` : "HEAD";
  switch (ui.compareMode) {
    case "worktreeIndex":
      return { left: "暂存区 (Index)", right: "当前工作区" };
    case "indexHead":
      return { left: head, right: "暂存区 (Index)" };
    case "localUpstream":
      return { left: status.value?.upstream ?? "上游", right: "本地 HEAD" };
    default:
      return { left: head, right: "当前工作区" };
  }
});

/** 改动文件分组：按风险从高到低（布局规范 5.3），空分组隐藏。 */
const groups = computed(() => {
  const order: { id: string; title: string; swatch: string }[] = [
    { id: "conflict", title: "冲突", swatch: "var(--status-danger-bar)" },
    { id: "both", title: "暂存后又修改", swatch: "var(--status-dirty-bar)" },
    { id: "staged", title: "已暂存", swatch: "var(--status-clean-bar)" },
    { id: "unstaged", title: "未暂存", swatch: "var(--status-dirty-bar)" },
    { id: "untracked", title: "未跟踪", swatch: "var(--status-neutral-bar)" },
    { id: "upstream", title: "与上游不同", swatch: "var(--status-sync-bar)" },
  ];
  return order
    .map((group) => ({ ...group, files: files.value.filter((file) => file.area === group.id) }))
    .filter((group) => group.files.length > 0);
});
const orderedFiles = computed(() => groups.value.flatMap((group) => group.files));

const showEditor = computed(() => textDiff.value !== null && !diffError.value);
const showImage = computed(() => imageDiff.value !== null && !diffLoading.value && !diffError.value);

function codeClass(code: string): string {
  if (code === "??" || code === "S") {
    return "";
  }
  if (code.includes("U") || code === "AA" || code === "DD") {
    return "conflict";
  }
  const letter = code[0];
  if (letter === "A") {
    return "add";
  }
  if (letter === "D") {
    return "del";
  }
  if (letter === "R" || letter === "C") {
    return "rename";
  }
  if (letter === "M" || letter === "T") {
    return "mod";
  }
  return "";
}

function isSelected(file: FileChange): boolean {
  return selected.value?.path === file.path && selected.value?.area === file.area;
}

async function loadList() {
  const ticket = ++listTicket;
  const path = props.path;
  const currentMode = ui.compareMode;
  listLoading.value = true;
  listError.value = null;
  try {
    const report = await api.listRepoChanges(path, currentMode);
    if (ticket !== listTicket || props.path !== path || ui.compareMode !== currentMode) {
      return;
    }
    files.value = report.files;
    const previous = selected.value;
    const still = previous
      ? report.files.find((file) => file.path === previous.path && file.area === previous.area) ??
        report.files.find((file) => file.path === previous.path)
      : undefined;
    if (still) {
      selected.value = still;
      await loadDiff(still);
    } else {
      selected.value = null;
      diff.value = null;
      diffError.value = null;
      chunkTotal.value = 0;
      chunkIndex.value = 0;
      // 打开仓库时自动选中风险最高分组里的第一个文件，省去一次点击。
      if (orderedFiles.value.length > 0) {
        choose(orderedFiles.value[0]);
      }
    }
  } catch (error) {
    if (ticket === listTicket) {
      files.value = [];
      listError.value = messageOf(error);
    }
  } finally {
    if (ticket === listTicket) {
      listLoading.value = false;
    }
  }
}

async function loadDiff(file: FileChange) {
  const ticket = ++diffTicket;
  const path = props.path;
  const currentMode = ui.compareMode;
  diffLoading.value = true;
  diffError.value = null;
  try {
    const result = await api.loadFileDiff(path, currentMode, file.path, file.oldPath);
    if (ticket !== diffTicket || props.path !== path || ui.compareMode !== currentMode) {
      return;
    }
    if (selected.value?.path !== file.path) {
      return;
    }
    diff.value = result;
  } catch (error) {
    if (ticket === diffTicket) {
      diff.value = null;
      diffError.value = messageOf(error);
      chunkTotal.value = 0;
      chunkIndex.value = 0;
    }
  } finally {
    if (ticket === diffTicket) {
      diffLoading.value = false;
    }
  }
}

function choose(file: FileChange) {
  selected.value = file;
  chunkTotal.value = 0;
  chunkIndex.value = 0;
  void loadDiff(file);
}

function retryDiff() {
  if (selected.value) {
    void loadDiff(selected.value);
  }
}

async function focusChunk(index: number) {
  chunkIndex.value = index;
  await nextTick();
  editor.value?.scrollTo(index - 1);
  // 跳转高亮闪一次（交互细则 1.4）。
  hunkFlash.value = false;
  await nextTick();
  hunkFlash.value = true;
}

function onChunks(count: number) {
  chunkTotal.value = count;
  if (count === 0) {
    chunkIndex.value = 0;
    return;
  }
  const next = chunkIndex.value >= 1 && chunkIndex.value <= count ? chunkIndex.value : 1;
  void focusChunk(next);
}

function step(delta: number) {
  const next = chunkIndex.value + delta;
  if (next < 1 || next > chunkTotal.value) {
    return;
  }
  void focusChunk(next);
}

function stepFile(delta: number) {
  const list = orderedFiles.value;
  if (list.length === 0) {
    return;
  }
  const index = selected.value ? list.findIndex((file) => isSelected(file)) : -1;
  const next = list[Math.min(list.length - 1, Math.max(0, index + delta))];
  if (next && !isSelected(next)) {
    choose(next);
  }
}

/* 快捷键：[ / ] 跳转差异，1~4 切换比较方式，详情内焦点时 ↑/↓ 切换文件。 */
function onKeyDown(event: KeyboardEvent) {
  const target = event.target as HTMLElement | null;
  if (target && ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName)) {
    return;
  }
  if (ui.settingsId || ui.folderDialog || ui.nameDialog || ui.confirmDeleteId || ui.confirmRemovePath) {
    return;
  }
  if (event.ctrlKey || event.altKey || event.metaKey) {
    return;
  }
  if (event.key === "[" || event.key === "]") {
    event.preventDefault();
    step(event.key === "[" ? -1 : 1);
    return;
  }
  const mode = modes.value.find((item) => item.key === event.key);
  if (mode) {
    event.preventDefault();
    ui.compareMode = mode.id;
    return;
  }
  if ((event.key === "ArrowDown" || event.key === "ArrowUp") && document.activeElement?.closest(".detail-panel")) {
    event.preventDefault();
    stepFile(event.key === "ArrowDown" ? 1 : -1);
  }
}

onMounted(() => window.addEventListener("keydown", onKeyDown));
onUnmounted(() => window.removeEventListener("keydown", onKeyDown));

watch(
  () => props.path,
  () => {
    ui.compareMode = "worktreeHead";
    selected.value = null;
    diff.value = null;
    files.value = [];
  },
);

watch(canUpstream, (ok) => {
  if (!ok && ui.compareMode === "localUpstream") {
    ui.compareMode = "worktreeHead";
  }
});

// 窄模式优先内联 diff，仍允许手动切回并排（布局规范 5.1）。
// 只有因变窄而自动切成内联的，在面板拉宽后才自动恢复并排；用户手动选的布局不动。
let autoInlined = false;
watch(
  narrow,
  (isNarrow) => {
    if (isNarrow && ui.diffLayout === "side") {
      ui.diffLayout = "inline";
      autoInlined = true;
    } else if (!isNarrow && autoInlined) {
      ui.diffLayout = "side";
      autoInlined = false;
    }
  },
  { immediate: true },
);

watch(
  () => [props.path, ui.compareMode, ui.refreshedAt] as const,
  () => {
    void loadList();
  },
  { immediate: true },
);
</script>

<template>
  <aside class="detail-panel" :class="{ narrow }" :style="{ width: `${width}px` }">
    <header class="detail-header">
      <div class="detail-title-row">
        <div class="detail-title">
          <strong :title="path">{{ status?.name ?? repoName(path) }}</strong>
          <span v-if="status?.changedSinceAck" class="badge badge-new">新变化</span>
        </div>
        <div class="detail-tools">
          <!-- 不自动消除新变化标记，由用户看完后手动确认（布局规范 2.2） -->
          <button v-if="status?.changedSinceAck" type="button" class="btn btn-sm btn-new" @click="ackRepos([path])">
            <Check :size="12" :stroke-width="2" />标记已查看
          </button>
          <button type="button" class="btn btn-ghost btn-icon" title="在 VS Code 中打开" @click="openVscode(path)">
            <Code :size="14" :stroke-width="1.85" />
          </button>
          <button type="button" class="btn btn-ghost btn-icon" title="在终端打开" @click="openTerminal(path)">
            <Terminal :size="14" :stroke-width="1.85" />
          </button>
          <button type="button" class="btn btn-ghost btn-icon" title="在资源管理器中打开" @click="openFolder(path)">
            <Folder :size="14" :stroke-width="1.85" />
          </button>
          <button type="button" class="btn btn-ghost btn-icon" title="关闭（Esc）" @click="ui.detailPath = null">
            <X :size="15" :stroke-width="1.85" />
          </button>
        </div>
      </div>
      <div class="detail-meta">
        <span class="path" :title="path">{{ path }}</span>
      </div>
      <div class="compare-row">
        <div class="compare-tabs" role="tablist">
          <button
            v-for="item in modes"
            :key="item.id"
            type="button"
            class="compare-tab"
            :class="{ active: ui.compareMode === item.id }"
            :title="`快捷键 ${item.key}`"
            @click="ui.compareMode = item.id"
          >
            {{ item.label }}
          </button>
        </div>
        <div class="compare-direction">
          <span class="side" :title="direction.left">左：{{ direction.left }}</span>
          <span class="arrow">→</span>
          <span class="side" :title="direction.right">右：{{ direction.right }}</span>
        </div>
      </div>
      <p v-if="ui.compareMode === 'localUpstream'" class="mode-note">只读取本地已有的上游引用，不会自动 Fetch；想看最新远端请先「Fetch 全部」。</p>
    </header>

    <div class="detail-body">
      <div class="file-list-pane" :class="{ collapsed: narrow && fileListCollapsed }">
        <p v-if="listLoading && files.length === 0" class="file-list-note">正在读取改动…</p>
        <p v-else-if="listError" class="file-list-note warn">{{ listError }}</p>
        <template v-for="group in groups" :key="group.id">
          <div class="file-group-title">
            <span class="marks"><span class="swatch" :style="{ background: group.swatch }" />{{ group.title }}</span>
            <span class="mono">{{ group.files.length }}</span>
          </div>
          <button
            v-for="file in group.files"
            :key="`${group.id}:${file.path}`"
            type="button"
            class="file-item"
            :class="{ active: isSelected(file) }"
            :title="`${file.label}：${file.oldPath ? `${file.oldPath} → ${file.path}` : file.path}`"
            @click="choose(file)"
          >
            <span class="file-tag" :class="codeClass(file.code)">{{ file.code === "??" ? "?" : file.code[0] }}</span>
            <span class="file-path-txt">{{ file.path }}</span>
          </button>
        </template>
      </div>

      <div class="diff-pane">
        <div class="diff-toolbar">
          <div class="diff-file">
            <button
              v-if="narrow"
              type="button"
              class="btn btn-ghost btn-icon btn-sm"
              :title="fileListCollapsed ? '展开文件列表' : '折叠文件列表'"
              @click="fileListCollapsed = !fileListCollapsed"
            >
              <ChevronDown v-if="fileListCollapsed" :size="14" />
              <ChevronUp v-else :size="14" />
            </button>
            <span v-if="selected" class="name" :title="selected.path">{{ selected.path }}</span>
            <span v-else class="text-muted">{{ files.length }} 个改动文件</span>
          </div>
          <div v-if="showEditor" class="diff-nav">
            <div class="mode-seg">
              <button type="button" :class="{ active: ui.diffLayout === 'side' }" @click="ui.diffLayout = 'side'">并排</button>
              <button type="button" :class="{ active: ui.diffLayout === 'inline' }" @click="ui.diffLayout = 'inline'">内联</button>
            </div>
            <button type="button" class="btn btn-sm" :disabled="chunkIndex <= 1" title="上一处（[）" @click="step(-1)">
              <ArrowUp :size="12" />上一处
            </button>
            <span class="count">{{ chunkTotal ? `${chunkIndex} / ${chunkTotal} 处差异` : "没有差异" }}</span>
            <button type="button" class="btn btn-sm" :disabled="chunkIndex >= chunkTotal" title="下一处（]）" @click="step(1)">
              <ArrowDown :size="12" />下一处
            </button>
          </div>
        </div>

        <!-- 特殊与空状态（布局规范 5.4） -->
        <div v-if="!listLoading && !listError && files.length === 0" class="state-card ok">
          <CircleCheck :size="28" :stroke-width="1.5" />
          <h4>当前比较视角下无差异</h4>
          <p v-if="ui.compareMode !== 'worktreeHead'">可以切回「工作区 vs HEAD」查看所有未提交改动。</p>
          <p v-else>工作区与 HEAD 一致，没有未提交的改动。</p>
          <button v-if="ui.compareMode !== 'worktreeHead'" type="button" class="btn" @click="ui.compareMode = 'worktreeHead'">
            切回 工作区 vs HEAD
          </button>
        </div>
        <div v-else-if="!selected && files.length > 0" class="state-card muted">
          <FileCode :size="28" :stroke-width="1.5" />
          <h4>选择一个文件查看差异</h4>
          <p>文本和图片都只在点开后加载。</p>
        </div>
        <div v-else-if="diffLoading && !textDiff" class="state-card muted">
          <RefreshCw :size="28" :stroke-width="1.5" class="spin" />
          <h4>正在读取差异…</h4>
        </div>
        <div v-else-if="diffError" class="state-card error">
          <TriangleAlert :size="28" :stroke-width="1.5" />
          <h4>读取失败</h4>
          <p>{{ diffError }}</p>
          <button type="button" class="btn" @click="retryDiff">重试读取</button>
        </div>
        <div v-else-if="diff && diff.kind === 'tooLarge'" class="state-card muted">
          <FileCode :size="28" :stroke-width="1.5" />
          <h4>文件体积较大，默认不渲染</h4>
          <p>{{ diff.message || "为保持流畅，已跳过预览。" }}</p>
          <button type="button" class="btn" @click="openVscode(path)"><Code :size="13" />在 VS Code 中打开</button>
        </div>
        <div v-else-if="diff && diff.kind !== 'text' && diff.kind !== 'image'" class="state-card muted">
          <FileCode :size="28" :stroke-width="1.5" />
          <h4>{{ selected?.path.split(/[\\/]/).pop() }} · 二进制文件</h4>
          <p>{{ diff.message || "二进制文件不作为文本显示。" }}</p>
          <button type="button" class="btn" @click="openFolder(path)"><Folder :size="13" />在资源管理器中定位</button>
        </div>
        <ImageCompare
          v-else-if="showImage && imageDiff"
          :left="imageDiff.leftImage"
          :right="imageDiff.rightImage"
          :media-type="imageDiff.mediaType || 'application/octet-stream'"
          :left-missing="imageDiff.leftMissing"
          :right-missing="imageDiff.rightMissing"
          :left-title="direction.left"
          :right-title="direction.right"
        />
        <DiffEditor
          v-show="showEditor"
          v-if="textDiff"
          ref="editor"
          :class="{ 'hunk-flash': hunkFlash }"
          :left="textDiff.leftText || ''"
          :right="textDiff.rightText || ''"
          :filename="textDiff.path"
          :layout="ui.diffLayout"
          :hunks="textDiff.hunks"
          @chunks="onChunks"
          @animationend="hunkFlash = false"
        />
      </div>
    </div>
  </aside>
</template>
