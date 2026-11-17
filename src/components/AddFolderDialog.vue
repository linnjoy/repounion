<script setup lang="ts">
import { computed, ref } from "vue";
import * as api from "../api";
import { commitFolder, messageOf, ui } from "../state";
import type { ScanReport, Tone } from "../types";

const path = ref("");
const scanning = ref(false);
const submitting = ref(false);
const error = ref("");
const report = ref<ScanReport | null>(null);
const checked = ref<Record<string, boolean>>({});
const remember = ref(true);

const selectedCount = computed(() => {
  if (!report.value) {
    return 0;
  }
  return report.value.entries.filter((entry) => !entry.alreadyInWorkspace && checked.value[entry.path]).length;
});

const canSubmit = computed(() => !!report.value && !scanning.value && !submitting.value && (selectedCount.value > 0 || remember.value));

function badgeClass(tone: Tone, already: boolean): string {
  if (already) {
    return "badge";
  }
  return `badge ${tone}`;
}

async function browse() {
  error.value = "";
  try {
    const picked = await api.pickDirectory("选择要扫描的目录");
    if (!picked) {
      return;
    }
    path.value = picked;
    await scan();
  } catch (err) {
    error.value = messageOf(err);
  }
}

async function scan() {
  const workspaceId = ui.activeWorkspaceId;
  const target = path.value.trim();
  if (!workspaceId || !target) {
    error.value = "请先填写目录";
    return;
  }
  scanning.value = true;
  error.value = "";
  report.value = null;
  try {
    const result = await api.scanDirectory(workspaceId, target);
    report.value = result;
    const next: Record<string, boolean> = {};
    for (const entry of result.entries) {
      next[entry.path] = !entry.alreadyInWorkspace;
    }
    checked.value = next;
  } catch (err) {
    error.value = messageOf(err);
  } finally {
    scanning.value = false;
  }
}

async function submit() {
  if (!report.value || !canSubmit.value) {
    return;
  }
  submitting.value = true;
  error.value = "";
  const selected = report.value.entries
    .filter((entry) => !entry.alreadyInWorkspace && checked.value[entry.path])
    .map((entry) => entry.path);
  const failure = await commitFolder(report.value.root, selected, remember.value);
  submitting.value = false;
  if (failure) {
    error.value = failure;
  }
}
</script>

<template>
  <div class="overlay" @mousedown.self="ui.folderDialog = false">
    <form class="dialog" @submit.prevent="submit">
      <div class="dialog-h">
        <h2>从目录添加仓库</h2>
        <p>递归查找该目录下的 Git 仓库，确认后再加入当前工作区。</p>
      </div>
      <div class="dialog-b">
        <div class="field-label">目录</div>
        <div class="path-field">
          <input v-model="path" type="text" spellcheck="false" placeholder="粘贴路径，或点击浏览" @keydown.enter.prevent="scan" />
          <button type="button" class="btn" @click="browse">浏览</button>
          <button type="button" class="btn" :disabled="scanning || !path.trim()" @click="scan">
            {{ scanning ? "扫描中…" : "扫描" }}
          </button>
        </div>
        <p v-if="error" class="scan-error">{{ error }}</p>
        <p v-if="report" class="scan-note">
          在该目录下找到 <strong>{{ report.entries.length }}</strong> 个仓库，其中
          <strong>{{ report.entries.filter((entry) => entry.alreadyInWorkspace).length }}</strong>
          个已在本工作区，已默认勾选其余
          <strong>{{ report.entries.filter((entry) => !entry.alreadyInWorkspace).length }}</strong> 个。
        </p>
        <div v-if="report && report.entries.length" class="found">
          <label v-for="entry in report.entries" :key="entry.path" class="found-row" :class="{ disabled: entry.alreadyInWorkspace }">
            <input v-model="checked[entry.path]" type="checkbox" :disabled="entry.alreadyInWorkspace" />
            <span>
              <span class="name">{{ entry.name }}</span>
              <div class="repo-path" :title="entry.path">{{ entry.path }}</div>
            </span>
            <span class="branch">{{ entry.branch }}</span>
            <span :class="badgeClass(entry.tone, entry.alreadyInWorkspace)">
              {{ entry.alreadyInWorkspace ? "已在工作区" : entry.badge }}
            </span>
          </label>
        </div>
        <label class="opt">
          <input v-model="remember" type="checkbox" />
          <span>
            将此目录记为来源
            <div class="hint">以后打开该工作区时自动发现新仓库，不会把你手动移除的仓库再加回来。</div>
          </span>
        </label>
      </div>
      <div class="dialog-f">
        <button type="button" class="btn" @click="ui.folderDialog = false">取消</button>
        <button type="submit" class="btn primary" :disabled="!canSubmit">
          {{ submitting ? "正在添加…" : selectedCount > 0 ? `添加 ${selectedCount} 个仓库` : "记为来源" }}
        </button>
      </div>
    </form>
  </div>
</template>
