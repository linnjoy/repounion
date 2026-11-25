<script setup lang="ts">
import { computed } from "vue";
import { openFolder, ui } from "../state";
import { wrapUpItems, type RepoStatus, type WrapUpItem } from "../types";

const props = defineProps<{ paths: string[] }>();
const emit = defineEmits<{ close: [] }>();

interface Group {
  status: RepoStatus;
  items: WrapUpItem[];
}

/** 只列当前工作区里还没收尾的仓库；保护分支风险排最前，其余按仓库名。 */
const groups = computed<Group[]>(() => {
  const owned = new Set(props.paths);
  return ui.statuses
    .filter((status) => owned.has(status.path))
    .map((status) => ({ status, items: wrapUpItems(status) }))
    .filter((group) => group.items.length > 0)
    .sort((left, right) => {
      const risk = Number(right.status.protectedRisk) - Number(left.status.protectedRisk);
      return risk !== 0 ? risk : left.status.name.localeCompare(right.status.name, "zh-CN");
    });
});

const total = computed(() => groups.value.reduce((sum, group) => sum + group.items.length, 0));

function openDetail(path: string) {
  ui.detailPath = path;
  emit("close");
}
</script>

<template>
  <div class="overlay" @mousedown.self="emit('close')">
    <div class="dialog wrapup" role="dialog" aria-modal="true">
      <div class="dialog-h">
        <h2>收工检查</h2>
        <p v-if="groups.length">
          {{ groups.length }} 个仓库共 {{ total }} 项还没收尾。数据来自最近一次刷新，只读本地，不联网。
        </p>
        <p v-else>当前工作区没有未提交、未推送、stash 或保护分支上的改动。</p>
      </div>
      <div class="dialog-b wrapup-body">
        <section v-for="group in groups" :key="group.status.path" class="wrapup-repo">
          <div class="wrapup-repo-h">
            <strong>{{ group.status.name }}</strong>
            <span class="branch">{{ group.status.branch }}</span>
            <span class="wrapup-actions">
              <button type="button" class="linkish" @click="openDetail(group.status.path)">查看改动</button>
              <button type="button" class="linkish" @click="openFolder(group.status.path)">打开目录</button>
            </span>
          </div>
          <ul>
            <li v-for="(item, index) in group.items" :key="index" :class="`wrapup-${item.kind}`">
              <code v-if="item.target && item.kind !== 'stash'">{{ item.target }}</code>
              {{ item.text }}
            </li>
          </ul>
        </section>
      </div>
      <div class="dialog-f">
        <button type="button" class="btn primary" @click="emit('close')">关闭</button>
      </div>
    </div>
  </div>
</template>
