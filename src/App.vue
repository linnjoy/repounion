<script setup lang="ts">
import { getCurrentWindow } from "@tauri-apps/api/window";
import { CircleCheck, Plus } from "lucide-vue-next";
import { computed, onMounted, onUnmounted } from "vue";
import AddFolderDialog from "./components/AddFolderDialog.vue";
import AppIcon from "./components/AppIcon.vue";
import ConfirmDialog from "./components/ConfirmDialog.vue";
import NameDialog from "./components/NameDialog.vue";
import SettingsDialog from "./components/SettingsDialog.vue";
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
const settingsWorkspace = computed(() => ui.workspaces.find((item) => item.id === ui.settingsId) ?? null);
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

/** 首次使用：「创建工作区并扫描目录」先建工作区，建好后自动打开扫描弹窗。 */
function startFirstRun(thenScan: boolean) {
  ui.nameDialog = { mode: "create", id: "", name: "", thenScan };
}

function onPointerDown(event: MouseEvent) {
  const target = event.target as HTMLElement | null;
  if (!target) {
    return;
  }
  if (target.closest(".row-menu-popover") || target.closest(".more") || target.closest(".menu-trigger")) {
    return;
  }
  ui.menu = null;
  ui.wsMenuId = null;
}

/**
 * Esc 逐层退出（交互细则 1.1）：确认弹窗 → 其他弹窗 → 设置弹窗 → 弹出菜单 → 详情面板 → 清空搜索。
 * F5 刷新本地状态，正在刷新时忽略重复触发。这两个键在输入框里也生效。
 */
function onKeyDown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    if (ui.confirmDeleteId || ui.confirmRemovePath) {
      ui.confirmDeleteId = null;
      ui.confirmRemovePath = null;
    } else if (ui.nameDialog || ui.folderDialog) {
      ui.nameDialog = null;
      ui.folderDialog = false;
    } else if (ui.settingsId) {
      ui.settingsId = null;
    } else if (ui.menu) {
      ui.menu = null;
    } else if (ui.detailPath) {
      ui.detailPath = null;
    } else if (ui.query) {
      ui.query = "";
    }
    return;
  }
  if (event.key === "F5") {
    event.preventDefault();
    if (workspace.value) {
      void refresh();
    }
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
  <div class="app-shell">
    <Sidebar />
    <WorkspaceBoard v-if="workspace" :workspace="workspace" />
    <main v-else class="first-run-stage">
      <!-- 首次使用空态（布局规范 6.4） -->
      <div v-if="!ui.booting" class="first-run-box">
        <AppIcon :size="40" />
        <h2>开始监控你的本地 Git 仓库</h2>
        <p>创建工作区并添加本地目录，随时掌握未收尾改动、保护分支风险与 AI agent 带来的仓库变化。</p>
        <div class="actions">
          <button type="button" class="btn btn-primary" @click="startFirstRun(true)">
            <Plus :size="14" :stroke-width="2" />创建工作区并扫描目录
          </button>
          <button type="button" class="btn" @click="startFirstRun(false)">仅新建空白工作区</button>
        </div>
      </div>
    </main>
  </div>

  <SettingsDialog v-if="settingsWorkspace" :workspace="settingsWorkspace" @close="ui.settingsId = null" />
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

  <div class="toast-stack" aria-live="polite">
    <!-- key 绑定文本：连续两条提示时重新播放弹入动画 -->
    <div v-if="ui.toast" :key="ui.toast" class="toast-item"><CircleCheck :size="14" />{{ ui.toast }}</div>
  </div>
</template>
