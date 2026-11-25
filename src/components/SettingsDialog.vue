<script setup lang="ts">
/**
 * 工作区设置弹窗（布局规范 6.3，宽 540px）。
 * 四个区块：保护分支规则、来源目录、自动扫描、排除清单；顶部可改名，底部可删除工作区。
 * 每项修改立即保存，不需要再点「保存」。
 */
import { Plus, RefreshCw, X } from "lucide-vue-next";
import { computed, ref } from "vue";
import { renameWorkspace, rescan, setAutoScan, setProtectedBranches, ui } from "../state";
import type { Workspace } from "../types";

/** 与后端 core/src/model.rs 的 default_protected_branches 保持一致。 */
const DEFAULT_PROTECTED = ["main", "master", "release/*"];

const props = defineProps<{ workspace: Workspace }>();
const emit = defineEmits<{ close: [] }>();

const name = ref(props.workspace.name);
const nameError = ref("");
const newPattern = ref("");
const patternError = ref("");
const busySource = ref<string | null>(null);

const excluded = computed(() =>
  props.workspace.sources.flatMap((source) => source.excluded.map((path) => ({ source: source.path, path }))),
);

async function saveName() {
  const next = name.value.trim();
  if (!next || next === props.workspace.name) {
    name.value = props.workspace.name;
    return;
  }
  nameError.value = (await renameWorkspace(props.workspace.id, next)) ?? "";
}

async function savePatterns(patterns: string[]) {
  patternError.value = (await setProtectedBranches(props.workspace.id, patterns)) ?? "";
}

async function addPattern() {
  const value = newPattern.value.trim();
  if (!value) {
    return;
  }
  if (props.workspace.protectedBranches.includes(value)) {
    newPattern.value = "";
    return;
  }
  await savePatterns([...props.workspace.protectedBranches, value]);
  if (!patternError.value) {
    newPattern.value = "";
  }
}

function removePattern(pattern: string) {
  void savePatterns(props.workspace.protectedBranches.filter((item) => item !== pattern));
}

function restoreDefaults() {
  void savePatterns(DEFAULT_PROTECTED);
}

async function toggleAutoScan(sourceId: string, event: Event) {
  const input = event.target as HTMLInputElement;
  const error = await setAutoScan(props.workspace.id, sourceId, input.checked);
  if (error) {
    input.checked = !input.checked;
    ui.banner = error;
  }
}

async function rescanSource(sourceId: string) {
  busySource.value = sourceId;
  await rescan(props.workspace.id, sourceId);
  busySource.value = null;
}

function addSource() {
  emit("close");
  ui.folderDialog = true;
}

function askDelete() {
  ui.confirmDeleteId = props.workspace.id;
}
</script>

<template>
  <div class="modal-backdrop" @mousedown.self="emit('close')">
    <div class="modal-dialog settings" role="dialog" aria-modal="true" aria-labelledby="settings-title">
      <div class="modal-head">
        <h3 id="settings-title">工作区设置</h3>
        <button type="button" class="btn btn-ghost btn-icon" title="关闭（Esc）" @click="emit('close')"><X :size="15" /></button>
      </div>
      <div class="modal-body">
        <div>
          <label class="field-label" for="ws-name">名称</label>
          <input id="ws-name" v-model="name" class="text-input" type="text" maxlength="80" @blur="saveName" @keydown.enter.prevent="saveName" />
          <p v-if="nameError" class="form-error">{{ nameError }}</p>
        </div>

        <section class="setting-block">
          <div class="setting-head">
            <span class="setting-label">保护分支规则</span>
            <button type="button" class="btn btn-ghost btn-sm" @click="restoreDefaults">恢复默认</button>
          </div>
          <p class="setting-desc">
            当前分支命中规则，且有未提交改动或未推送提交时，列表会红色警告。<code>*</code> 匹配任意字符，例如 <code>release/*</code>。
          </p>
          <div class="tag-list">
            <span v-for="pattern in workspace.protectedBranches" :key="pattern" class="tag-chip">
              {{ pattern }}
              <button type="button" :title="`移除 ${pattern}`" @click="removePattern(pattern)"><X :size="12" /></button>
            </span>
            <span v-if="workspace.protectedBranches.length === 0" class="setting-desc">尚未设置任何规则</span>
            <span class="tag-add">
              <input v-model="newPattern" class="text-input" type="text" placeholder="添加规则…" spellcheck="false" @keydown.enter.prevent="addPattern" />
              <button type="button" class="btn btn-sm btn-icon" title="添加" @click="addPattern"><Plus :size="12" /></button>
            </span>
          </div>
          <p v-if="patternError" class="form-error">{{ patternError }}</p>
        </section>

        <section class="setting-block">
          <div class="setting-head">
            <span class="setting-label">来源目录与自动扫描</span>
            <button type="button" class="btn btn-sm" @click="addSource"><Plus :size="12" />添加目录</button>
          </div>
          <p class="setting-desc">打开工作区时，自动发现来源目录下新增的 Git 仓库。不会监视文件，也不会删除磁盘上的任何内容。</p>
          <div v-for="source in workspace.sources" :key="source.id" class="source-line">
            <span class="path" :title="source.path">{{ source.path }}</span>
            <span class="mono text-muted">{{ source.repos.length }}</span>
            <label class="switch" title="打开工作区时自动扫描">
              <input type="checkbox" :checked="source.autoScan" @change="toggleAutoScan(source.id, $event)" />
              <span class="track" />
              自动扫描
            </label>
            <button
              type="button"
              class="btn btn-ghost btn-icon btn-sm"
              title="立即重新扫描"
              :disabled="busySource === source.id"
              @click="rescanSource(source.id)"
            >
              <RefreshCw :size="13" :class="{ spin: busySource === source.id }" />
            </button>
          </div>
          <p v-if="workspace.sources.length === 0" class="setting-desc">还没有来源目录。</p>
        </section>

        <section class="setting-block">
          <span class="setting-label">排除清单</span>
          <p class="setting-desc">从工作区移除的来源仓库会记在这里，自动扫描不会再把它们加回来。</p>
          <div class="tag-list">
            <span v-for="item in excluded" :key="item.path" class="tag-chip" :title="`来源：${item.source}`">{{ item.path }}</span>
            <span v-if="excluded.length === 0" class="setting-desc">没有排除的仓库</span>
          </div>
        </section>
      </div>
      <div class="modal-foot">
        <button type="button" class="btn btn-danger" @click="askDelete">删除工作区</button>
        <div class="right">
          <button type="button" class="btn btn-primary" @click="emit('close')">完成</button>
        </div>
      </div>
    </div>
  </div>
</template>
