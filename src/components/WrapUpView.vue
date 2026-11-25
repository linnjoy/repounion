<script setup lang="ts">
/**
 * 收工检查视图（布局规范 3）：按风险分区列出所有还没收尾的事项，粒度到分支。
 * 批量推送（勾选 + 「推送 N 个分支」）属于 P2 写操作，接入后在每行左侧加勾选框。
 */
import { Archive, ArrowUp, CircleCheck, FolderGit2, GitBranch, ShieldAlert } from "lucide-vue-next";
import { computed } from "vue";
import { openTerminal, ui } from "../state";
import { wrapUpItems, type RepoStatus, type WrapUpCategory, type WrapUpItem } from "../types";

const props = defineProps<{ statuses: RepoStatus[] }>();

const sections: { category: WrapUpCategory; title: string }[] = [
  { category: "protected", title: "保护分支上的风险改动" },
  { category: "dirty", title: "未提交的工作区改动" },
  { category: "unpushed", title: "所有分支上的未推送提交（分支粒度）" },
  { category: "worktree", title: "其他 worktree 中的未提交改动" },
  { category: "stash", title: "尚未清理的 Stash" },
];

const items = computed<WrapUpItem[]>(() =>
  props.statuses
    .flatMap((status) => wrapUpItems(status))
    .sort((left, right) => left.repo.name.localeCompare(right.repo.name, "zh-CN")),
);

const repoCount = computed(() => new Set(items.value.map((item) => item.repo.path)).size);
const unpushedCount = computed(() => items.value.filter((item) => item.category === "unpushed").length);

function sectionItems(category: WrapUpCategory): WrapUpItem[] {
  return items.value.filter((item) => item.category === category);
}

function inspect(path: string) {
  ui.detailPath = path;
}
</script>

<template>
  <div class="wrapup-header">
    <div>
      <strong>收工检查 · 分支粒度未收尾清单</strong>
      <span class="sub">共 {{ items.length }} 项待处理（覆盖 {{ repoCount }} 个仓库，其中 {{ unpushedCount }} 个分支有未推送提交）</span>
    </div>
  </div>
  <div class="list-viewport">
    <div v-if="items.length === 0" class="state-card ok">
      <CircleCheck :size="28" :stroke-width="1.5" />
      <h4>当前工作区已全部收工</h4>
      <p>没有未提交改动、未推送分支、保护分支风险、遗留 stash 或其他 worktree 的改动。</p>
    </div>

    <template v-for="section in sections" :key="section.category">
      <template v-if="sectionItems(section.category).length">
        <div class="group-header static">
          <div class="group-header-left">
            <ShieldAlert v-if="section.category === 'protected'" :size="14" :stroke-width="1.85" style="color: var(--status-danger-fg)" />
            <span v-else-if="section.category === 'dirty'" style="color: var(--status-dirty-fg)">●</span>
            <ArrowUp v-else-if="section.category === 'unpushed'" :size="14" :stroke-width="1.85" style="color: var(--status-sync-fg)" />
            <FolderGit2 v-else-if="section.category === 'worktree'" :size="14" :stroke-width="1.85" style="color: var(--status-dirty-fg)" />
            <Archive v-else :size="14" :stroke-width="1.85" />
            <strong>{{ section.title }}</strong>
            <span class="mono text-muted">({{ sectionItems(section.category).length }})</span>
          </div>
        </div>
        <div v-for="item in sectionItems(section.category)" :key="item.key" class="wrapup-row">
          <div class="wrapup-row-left">
            <span class="repo">{{ item.repo.name }}</span>
            <span v-if="item.target" class="branch-pill" :class="{ 'protected-warn': item.category === 'protected' }">
              <ShieldAlert v-if="item.category === 'protected'" :size="12" :stroke-width="1.85" />
              <FolderGit2 v-else-if="item.category === 'worktree'" :size="12" :stroke-width="1.85" />
              <GitBranch v-else :size="12" :stroke-width="1.85" />
              <span>{{ item.target }}</span>
            </span>
            <span v-if="item.otherBranch" class="badge badge-neutral">非当前分支</span>
            <template v-if="item.category === 'unpushed'">
              <span class="badge badge-sync mono"><ArrowUp :size="12" :stroke-width="1.85" />超前 {{ item.ahead }}</span>
              <span class="mono detail">{{ item.detail }}</span>
            </template>
            <span v-else class="detail" :title="item.detail">{{ item.detail }}</span>
          </div>
          <div class="wrapup-row-right">
            <button v-if="item.category === 'stash'" type="button" class="btn btn-sm" @click="openTerminal(item.repo.path)">
              在终端打开
            </button>
            <button v-else type="button" class="btn btn-sm" @click="inspect(item.repo.path)">查看 Diff</button>
          </div>
        </div>
      </template>
    </template>
  </div>
</template>
