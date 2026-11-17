<script setup lang="ts">
import { computed } from "vue";
import { selectWorkspace, ui } from "../state";
import type { Workspace } from "../types";

function countOf(workspace: Workspace): number {
  return workspace.sources.reduce((sum, source) => sum + source.repos.length, 0) + workspace.standaloneRepos.length;
}

const menuWorkspace = computed(() => ui.workspaces.find((item) => item.id === ui.wsMenuId) ?? null);

function dotClass(workspace: Workspace): string {
  if (workspace.id !== ui.activeWorkspaceId || ui.statuses.length === 0) {
    return "idle";
  }
  if (ui.statuses.some((item) => item.tone === "conflict")) {
    return "conflict";
  }
  if (ui.statuses.some((item) => item.tone === "dirty")) {
    return "alert";
  }
  if (ui.statuses.every((item) => item.tone === "clean")) {
    return "ok";
  }
  return "idle";
}

function toggleMenu(event: MouseEvent, workspace: Workspace) {
  const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
  if (ui.wsMenuId === workspace.id) {
    ui.wsMenuId = null;
    ui.menu = null;
    return;
  }
  ui.menu = null;
  ui.wsMenuId = workspace.id;
  ui.menu = { path: `ws:${workspace.id}`, x: rect.left, y: rect.bottom + 4 };
}

function rename(workspace: Workspace) {
  ui.wsMenuId = null;
  ui.menu = null;
  ui.nameDialog = { mode: "rename", id: workspace.id, name: workspace.name };
}

function askDelete(id: string) {
  ui.wsMenuId = null;
  ui.menu = null;
  ui.confirmDeleteId = id;
}
</script>

<template>
  <aside class="sidebar">
    <div class="side-head">工作区</div>
    <div class="ws-list">
      <div
        v-for="workspace in ui.workspaces"
        :key="workspace.id"
        class="ws-item"
        :class="{ active: workspace.id === ui.activeWorkspaceId }"
        role="button"
        tabindex="0"
        @click="selectWorkspace(workspace.id)"
        @keydown.enter.prevent="selectWorkspace(workspace.id)"
      >
        <span class="dot" :class="dotClass(workspace)" />
        <span class="name" :title="workspace.name" @dblclick.stop="rename(workspace)">{{ workspace.name }}</span>
        <span class="count">{{ countOf(workspace) }}</span>
        <button type="button" class="ws-more" title="工作区操作" @click.stop="toggleMenu($event, workspace)">⋯</button>
      </div>
      <p v-if="ui.workspaces.length === 0 && !ui.booting" class="loading-line">还没有工作区</p>
    </div>
    <div class="side-foot">
      <button type="button" class="ghost-btn" @click="ui.nameDialog = { mode: 'create', id: '', name: '' }">
        + 新建工作区
      </button>
    </div>
    <div
      v-if="ui.wsMenuId && ui.menu"
      class="menu"
      :style="{ left: `${ui.menu.x}px`, top: `${ui.menu.y}px` }"
    >
      <button v-if="menuWorkspace" type="button" @click="rename(menuWorkspace)">重命名</button>
      <button v-if="menuWorkspace" type="button" class="danger" @click="askDelete(menuWorkspace.id)">删除工作区</button>
    </div>
  </aside>
</template>
