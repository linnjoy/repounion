<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import * as api from "../api";
import { messageOf, ui } from "../state";
import type { CompareMode, FileChange, FileDiff } from "../types";
import { repoName } from "../types";
import DiffEditor from "./DiffEditor.vue";
import ImageCompare from "./ImageCompare.vue";

const props = defineProps<{ path: string; width: number }>();

const mode = ref<CompareMode>("worktreeHead");
const files = ref<FileChange[]>([]);
const selected = ref<FileChange | null>(null);
const diff = ref<FileDiff | null>(null);
const listError = ref<string | null>(null);
const diffError = ref<string | null>(null);
const listLoading = ref(false);
const diffLoading = ref(false);
const chunkTotal = ref(0);
const chunkIndex = ref(0);
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
  const items: { id: CompareMode; label: string }[] = [
    { id: "worktreeHead", label: "工作区 vs HEAD" },
    { id: "worktreeIndex", label: "工作区 vs 暂存" },
    { id: "indexHead", label: "暂存 vs HEAD" },
  ];
  if (canUpstream.value) {
    items.push({ id: "localUpstream", label: "本地 vs 上游" });
  }
  return items;
});

const groups = computed(() => {
  const order: { id: string; title: string }[] = [
    { id: "conflict", title: "冲突" },
    { id: "both", title: "暂存后又修改" },
    { id: "staged", title: "已暂存" },
    { id: "unstaged", title: "未暂存" },
    { id: "untracked", title: "未跟踪" },
    { id: "upstream", title: "与上游不同" },
  ];
  return order
    .map((group) => ({
      ...group,
      files: files.value.filter((file) => file.area === group.id),
    }))
    .filter((group) => group.files.length > 0);
});

const sideNames = computed(() => {
  switch (mode.value) {
    case "worktreeIndex":
      return ["暂存", "工作区"];
    case "indexHead":
      return ["HEAD", "暂存"];
    case "localUpstream":
      return ["上游", "本地"];
    default:
      return ["HEAD", "工作区"];
  }
});

const showEditor = computed(() => textDiff.value !== null && !diffError.value);
const showImage = computed(() => imageDiff.value !== null && !diffLoading.value && !diffError.value);

function codeClass(code: string): string {
  if (code === "??" || code === "S") {
    return "muted";
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
  return "muted";
}

function isSelected(file: FileChange): boolean {
  return selected.value?.path === file.path && selected.value?.area === file.area;
}

async function loadList() {
  const ticket = ++listTicket;
  const path = props.path;
  const currentMode = mode.value;
  listLoading.value = true;
  listError.value = null;
  try {
    const report = await api.listRepoChanges(path, currentMode);
    if (ticket !== listTicket || props.path !== path || mode.value !== currentMode) {
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
  const currentMode = mode.value;
  diffLoading.value = true;
  diffError.value = null;
  try {
    const result = await api.loadFileDiff(path, currentMode, file.path, file.oldPath);
    if (ticket !== diffTicket || props.path !== path || mode.value !== currentMode) {
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

async function focusChunk(index: number) {
  chunkIndex.value = index;
  await nextTick();
  editor.value?.scrollTo(index - 1);
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

watch(
  () => props.path,
  () => {
    mode.value = "worktreeHead";
    selected.value = null;
    diff.value = null;
    files.value = [];
  },
);

watch(canUpstream, (ok) => {
  if (!ok && mode.value === "localUpstream") {
    mode.value = "worktreeHead";
  }
});

watch(
  () => [props.path, mode.value, ui.refreshedAt] as const,
  () => {
    void loadList();
  },
  { immediate: true },
);
</script>

<template>
  <aside class="detail" :style="{ width: `${width}px` }">
    <header class="detail-h">
      <div class="detail-title">
        <strong :title="path">{{ status?.name ?? repoName(path) }}</strong>
        <span class="branch" :title="status?.branch">{{ status?.branch ?? "—" }}</span>
        <button type="button" class="icon-x" title="关闭" @click="ui.detailPath = null">×</button>
      </div>
      <p class="detail-sub">
        <span>{{ status?.workspaceLabel ?? "尚未刷新" }}</span>
        <span>{{ status?.remoteLabel ?? "—" }}</span>
      </p>
      <div class="mode-row">
        <button
          v-for="item in modes"
          :key="item.id"
          type="button"
          class="chip"
          :class="{ on: mode === item.id }"
          @click="mode = item.id"
        >
          {{ item.label }}
        </button>
      </div>
      <p v-if="mode === 'localUpstream'" class="mode-note">只读取本地已有的上游引用，不会自动 Fetch。</p>
    </header>

    <div class="detail-body">
      <div class="file-col">
        <p v-if="listLoading && files.length === 0" class="detail-empty">正在读取改动…</p>
        <p v-else-if="listError" class="detail-empty warn">{{ listError }}</p>
        <p v-else-if="files.length === 0" class="detail-empty">这个比较方式下没有改动文件。</p>
        <section v-for="group in groups" :key="group.id" class="file-group">
          <header>{{ group.title }} {{ group.files.length }}</header>
          <button
            v-for="file in group.files"
            :key="`${group.id}:${file.path}`"
            type="button"
            class="file-row"
            :class="{ on: isSelected(file) }"
            :title="file.oldPath ? `${file.oldPath} → ${file.path}` : file.path"
            @click="choose(file)"
          >
            <span class="code" :class="codeClass(file.code)">{{ file.code }}</span>
            <span class="file-text">
              <span class="file-path">{{ file.path }}</span>
              <span class="file-label">{{ file.label }}</span>
            </span>
          </button>
        </section>
      </div>

      <div class="diff-col">
        <div v-if="selected" class="diff-bar">
          <div class="diff-name" :title="selected.path">
            <strong>{{ selected.path }}</strong>
            <span>{{ sideNames[0] }} ↔ {{ sideNames[1] }}</span>
          </div>
          <div class="diff-actions">
            <template v-if="showEditor">
              <span v-if="diffLoading">正在读取…</span>
              <button type="button" :disabled="chunkIndex <= 1" @click="step(-1)">上一处</button>
              <button type="button" :disabled="chunkIndex >= chunkTotal" @click="step(1)">下一处</button>
              <span class="count">
                {{ chunkTotal ? `${chunkIndex} / ${chunkTotal} 处差异` : "没有差异" }}
              </span>
              <button type="button" class="chip" :class="{ on: ui.diffLayout === 'side' }" @click="ui.diffLayout = 'side'">
                并排
              </button>
              <button
                type="button"
                class="chip"
                :class="{ on: ui.diffLayout === 'inline' }"
                @click="ui.diffLayout = 'inline'"
              >
                内联
              </button>
            </template>
          </div>
        </div>

        <p v-if="!selected" class="detail-empty">选择左侧文件查看差异。文本和图片都只在点开后加载。</p>
        <p v-else-if="diffLoading && !textDiff" class="detail-empty">正在读取差异…</p>
        <p v-else-if="diffError" class="detail-empty warn">{{ diffError }}</p>
        <p v-else-if="diff && diff.kind !== 'text' && diff.kind !== 'image'" class="detail-empty">
          {{ diff.message || "无法预览这个文件。" }}
        </p>
        <ImageCompare
          v-else-if="showImage && imageDiff"
          :left="imageDiff.leftImage"
          :right="imageDiff.rightImage"
          :media-type="imageDiff.mediaType || 'application/octet-stream'"
          :left-missing="imageDiff.leftMissing"
          :right-missing="imageDiff.rightMissing"
          :left-title="sideNames[0]"
          :right-title="sideNames[1]"
        />
        <DiffEditor
          v-show="showEditor"
          v-if="textDiff"
          ref="editor"
          :left="textDiff.leftText || ''"
          :right="textDiff.rightText || ''"
          :filename="textDiff.path"
          :layout="ui.diffLayout"
          :hunks="textDiff.hunks"
          @chunks="onChunks"
        />
      </div>
    </div>
  </aside>
</template>
