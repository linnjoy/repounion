<script setup lang="ts">
import { onMounted, ref } from "vue";
import { saveProtectedBranches } from "../state";
import type { Workspace } from "../types";

const props = defineProps<{ workspace: Workspace }>();
const emit = defineEmits<{ cancel: [] }>();

const text = ref(props.workspace.protectedBranches.join("\n"));
const error = ref("");
const pending = ref(false);
const input = ref<HTMLTextAreaElement | null>(null);

onMounted(() => input.value?.focus());

async function onSubmit() {
  pending.value = true;
  error.value = "";
  const failure = await saveProtectedBranches(props.workspace.id, text.value);
  pending.value = false;
  if (failure) {
    error.value = failure;
  }
}
</script>

<template>
  <div class="overlay" @mousedown.self="emit('cancel')">
    <form class="dialog sm" @submit.prevent="onSubmit">
      <div class="dialog-h">
        <h2>保护分支</h2>
        <p>「{{ workspace.name }}」中，当前分支命中这些规则且有改动或未推送提交时，列表会警告。</p>
      </div>
      <div class="dialog-b">
        <label class="field-label" for="protected-branches">每行一条，<code>*</code> 匹配任意字符，如 <code>release/*</code></label>
        <textarea id="protected-branches" ref="input" v-model="text" rows="6" spellcheck="false" />
        <p v-if="error" class="scan-error">{{ error }}</p>
      </div>
      <div class="dialog-f">
        <button type="button" class="btn" @click="emit('cancel')">取消</button>
        <button type="submit" class="btn primary" :disabled="pending">保存</button>
      </div>
    </form>
  </div>
</template>
