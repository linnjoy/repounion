<script setup lang="ts">
import { getCurrentWindow } from "@tauri-apps/api/window";
import { computed, onMounted, onUnmounted } from "vue";
import AddFolderDialog from "./components/AddFolderDialog.vue";
import ConfirmDialog from "./components/ConfirmDialog.vue";
import NameDialog from "./components/NameDialog.vue";
import Sidebar from "./components/Sidebar.vue";
import WorkspaceBoard from "./components/WorkspaceBoard.vue";
import {
  activeWorkspace,
  boot,
  createWorkspace,
  deleteWorkspace,
  refresh,
  refreshOnFocus,
  removeRepo,
  renameWorkspace,
  ui,
} from "./state";
import { repoName } from "./types";

const workspace = computed(() => activeWorkspace());
const deleting = computed(() => ui.workspaces.find((item) => item.id === ui.confirmDeleteId) ?? null);
const removing = computed(() => {
  const path = ui.confirmRemovePath;
  if (!path) {
    return null;
  }
  return ui.statuses.find((item) => item.path === path)?.name ?? repoName(path);
});

function submitRename(name: string) {
  return renameWorkspace(ui.nameDialog?.id ?? "", name);
}

function onPointerDown(event: MouseEvent) {
  const target = event.target as HTMLElement | null;
  if (!target) {
    return;
  }
  if (target.closest(".menu") || target.closest(".more") || target.closest(".ws-more")) {
    return;
  }
  ui.menu = null;
  ui.wsMenuId = null;
}

function onKeyDown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    const dialogOpen =
      ui.folderDialog || ui.nameDialog || ui.menu || ui.wsMenuId || ui.confirmDeleteId || ui.confirmRemovePath;
    ui.folderDialog = false;
    ui.nameDialog = null;
    ui.menu = null;
    ui.wsMenuId = null;
    ui.confirmDeleteId = null;
    ui.confirmRemovePath = null;
    if (!dialogOpen) {
      ui.detailPath = null;
    }
  }
  if (event.key === "F5" && workspace.value) {
    event.preventDefault();
    void refresh();
  }
}

let stopFocusListener: (() => void) | null = null;
let unmounted = false;

onMounted(async () => {
  window.addEventListener("pointerdown", onPointerDown);
  window.addEventListener("keydown", onKeyDown);
  void boot();
  // 切回窗口时只读刷新本地状态（不联网），节流规则在 refreshOnFocus 里。
  const stop = await getCurrentWindow().onFocusChanged(({ payload: focused }) => {
    if (focused) {
      refreshOnFocus();
    }
  });
  // 注册是异步的，组件可能已先卸载，这时立即注销。
  if (unmounted) {
    stop();
  } else {
    stopFocusListener = stop;
  }
});

onUnmounted(() => {
  unmounted = true;
  window.removeEventListener("pointerdown", onPointerDown);
  window.removeEventListener("keydown", onKeyDown);
  stopFocusListener?.();
});
</script>

<template>
  <div class="shell">
    <Sidebar />
    <WorkspaceBoard v-if="workspace" :workspace="workspace" />
    <section v-else class="main">
      <div class="empty">
        <div class="empty-card">
          <h2>{{ ui.booting ? "正在打开…" : "还没有工作区" }}</h2>
          <p v-if="!ui.booting">先建一个工作区，再把仓库或整个目录加进来。RepoUnion 只看状态，不会替你提交或合并。</p>
          <div v-if="!ui.booting" class="empty-actions">
            <button type="button" class="btn primary" @click="ui.nameDialog = { mode: 'create', id: '', name: '' }">
              新建工作区
            </button>
          </div>
        </div>
      </div>
    </section>
  </div>

  <AddFolderDialog v-if="ui.folderDialog && workspace" />

  <NameDialog
    v-if="ui.nameDialog?.mode === 'create'"
    title="新建工作区"
    confirm-text="创建"
    initial=""
    :submit="createWorkspace"
    @cancel="ui.nameDialog = null"
  />
  <NameDialog
    v-if="ui.nameDialog?.mode === 'rename'"
    title="重命名工作区"
    confirm-text="保存"
    :initial="ui.nameDialog.name"
    :submit="submitRename"
    @cancel="ui.nameDialog = null"
  />

  <ConfirmDialog
    v-if="deleting"
    title="删除工作区"
    :body="`删除工作区「${deleting.name}」？只从 RepoUnion 移除，不会删除磁盘上的仓库。`"
    confirm-text="删除"
    danger
    @cancel="ui.confirmDeleteId = null"
    @confirm="deleteWorkspace(deleting.id)"
  />
  <ConfirmDialog
    v-if="ui.confirmRemovePath"
    title="从工作区移除"
    :body="`从工作区移除「${removing}」？不会删除磁盘上的文件。如果它来自来源目录，会记入排除清单，自动扫描不会再加回来。`"
    confirm-text="移除"
    danger
    @cancel="ui.confirmRemovePath = null"
    @confirm="removeRepo(ui.confirmRemovePath || '')"
  />

  <div v-if="ui.toast" class="toast">{{ ui.toast }}</div>
</template>
