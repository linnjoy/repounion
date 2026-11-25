<script setup lang="ts">
import { LoaderCircle, Search, X } from "lucide-vue-next";
import { computed, onMounted, ref } from "vue";
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
const input = ref<HTMLInputElement | null>(null);

onMounted(() => input.value?.focus());

const selectedCount = computed(() => {
  if (!report.value) {
    return 0;
  }
  return report.value.entries.filter((entry) => !entry.alreadyInWorkspace && checked.value[entry.path]).length;
});

const canSubmit = computed(() => !!report.value && !scanning.value && !submitting.value && (selectedCount.value > 0 || remember.value));

/** 扫描结果的状态徽章沿用列表的语义色。 */
function badgeClass(tone: Tone, already: boolean): string {
  if (already) {
    return "badge badge-neutral";
  }
  const map: Record<Tone, string> = {
    dirty: "badge-dirty",
    clean: "badge-clean",
    conflict: "badge-danger",
    unsynced: "badge-sync",
    noUpstream: "badge-neutral badge-dashed",
    error: "badge-danger badge-dashed",
  };
  return `badge ${map[tone]}`;
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
  <div class="modal-backdrop" @mousedown.self="ui.folderDialog = false">
    <form class="modal-dialog" role="dialog" aria-modal="true" aria-labelledby="scan-title" @submit.prevent="submit">
      <div class="modal-head">
        <h3 id="scan-title">扫描目录并添加仓库</h3>
        <button type="button" class="btn btn-ghost btn-icon" title="关闭（Esc）" @click="ui.folderDialog = false"><X :size="15" /></button>
      </div>
      <div class="modal-body">
        <p>递归查找该目录下的 Git 仓库，勾选确认后再加入当前工作区。会跳过 node_modules、target、dist、build 等目录里的嵌套仓库。</p>
        <div>
          <label class="field-label" for="scan-path">目录</label>
          <div class="path-field">
            <input
              id="scan-path"
              ref="input"
              v-model="path"
              type="text"
              spellcheck="false"
              placeholder="粘贴路径，或点击浏览"
              @keydown.enter.prevent="scan"
            />
            <button type="button" class="btn" @click="browse">浏览…</button>
            <button type="button" class="btn" :disabled="scanning || !path.trim()" @click="scan">
              <LoaderCircle v-if="scanning" :size="13" class="spin" />
              <Search v-else :size="13" />
              {{ scanning ? "扫描中…" : "扫描" }}
            </button>
          </div>
        </div>
        <p v-if="error" class="form-error">{{ error }}</p>
        <p v-if="report">
          找到 <strong class="mono">{{ report.entries.length }}</strong> 个仓库，其中
          <strong class="mono">{{ report.entries.filter((entry) => entry.alreadyInWorkspace).length }}</strong>
          个已在本工作区，已默认勾选其余
          <strong class="mono">{{ report.entries.filter((entry) => !entry.alreadyInWorkspace).length }}</strong> 个。
        </p>
        <div v-if="report && report.entries.length" class="found-list">
          <label v-for="entry in report.entries" :key="entry.path" class="found-row" :class="{ disabled: entry.alreadyInWorkspace }">
            <input v-model="checked[entry.path]" type="checkbox" :disabled="entry.alreadyInWorkspace" />
            <span style="min-width: 0">
              <span class="name">{{ entry.name }}</span>
              <div class="col-path" :title="entry.path">{{ entry.path }}</div>
            </span>
            <span class="branch-pill"><span>{{ entry.branch }}</span></span>
            <span :class="badgeClass(entry.tone, entry.alreadyInWorkspace)">
              {{ entry.alreadyInWorkspace ? "已在工作区" : entry.badge }}
            </span>
          </label>
        </div>
        <label class="check-line">
          <input v-model="remember" type="checkbox" />
          <span>
            将此目录记为来源
            <span class="hint">以后打开该工作区时自动发现新仓库，不会把你手动移除的仓库再加回来。</span>
          </span>
        </label>
      </div>
      <div class="modal-foot">
        <div class="right">
          <button type="button" class="btn" @click="ui.folderDialog = false">取消</button>
          <button type="submit" class="btn btn-primary" :disabled="!canSubmit">
            {{ submitting ? "正在添加…" : selectedCount > 0 ? `添加 ${selectedCount} 个仓库` : "记为来源" }}
          </button>
        </div>
      </div>
    </form>
  </div>
</template>
