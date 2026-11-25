<script setup lang="ts">
import { Plus, Settings } from "lucide-vue-next";
import { selectWorkspace, ui } from "../state";
import { setTheme, theme, type ThemePref } from "../theme";
import { isDirty, type Workspace } from "../types";
import AppIcon from "./AppIcon.vue";

function countOf(workspace: Workspace): number {
  return workspace.sources.reduce((sum, source) => sum + source.repos.length, 0) + workspace.standaloneRepos.length;
}

/**
 * 工作区圆点：有新变化为紫、有改动为橙、否则为绿。
 * 只有当前工作区有实时状态；其他工作区没有读取过，显示中性灰。
 */
function dotColor(workspace: Workspace): string {
  if (workspace.id !== ui.activeWorkspaceId || ui.statuses.length === 0) {
    return "var(--status-neutral-bar)";
  }
  if (ui.statuses.some((item) => item.changedSinceAck)) {
    return "var(--status-new-bar)";
  }
  if (ui.statuses.some(isDirty)) {
    return "var(--status-dirty-bar)";
  }
  return "var(--status-clean-bar)";
}

const themes: { id: ThemePref; label: string }[] = [
  { id: "system", label: "跟随" },
  { id: "light", label: "浅色" },
  { id: "dark", label: "深色" },
];

function createWorkspace() {
  ui.nameDialog = { mode: "create", id: "", name: "" };
}
</script>

<template>
  <aside class="sidebar">
    <div class="sidebar-brand">
      <div class="brand-left">
        <AppIcon :size="20" />
        <span>RepoUnion</span>
      </div>
      <button type="button" class="btn btn-ghost btn-icon" title="新建工作区" @click="createWorkspace">
        <Plus :size="16" :stroke-width="1.75" />
      </button>
    </div>

    <div class="sidebar-section">
      <div class="sidebar-heading">
        <span>工作区</span>
        <span class="mono">{{ ui.workspaces.length }}</span>
      </div>
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
        <div class="ws-item-left">
          <span class="ws-dot" :style="{ background: dotColor(workspace) }" />
          <span class="ws-name" :title="workspace.name">{{ workspace.name }}</span>
        </div>
        <div class="ws-right">
          <span class="ws-count mono">{{ countOf(workspace) }}</span>
          <button
            type="button"
            class="btn btn-ghost btn-icon btn-sm ws-gear"
            title="工作区设置"
            @click.stop="ui.settingsId = workspace.id"
          >
            <Settings :size="14" :stroke-width="1.75" />
          </button>
        </div>
      </div>
      <div v-if="ui.workspaces.length === 0 && !ui.booting" class="sidebar-empty">暂无工作区</div>
    </div>

    <div class="sidebar-footer">
      <div class="theme-seg" role="radiogroup" aria-label="界面主题">
        <button
          v-for="item in themes"
          :key="item.id"
          type="button"
          role="radio"
          :aria-checked="theme.pref === item.id"
          :class="{ active: theme.pref === item.id }"
          @click="setTheme(item.id)"
        >
          {{ item.label }}
        </button>
      </div>
    </div>
  </aside>
</template>
