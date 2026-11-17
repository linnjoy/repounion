<script setup lang="ts">
import { onMounted, ref } from "vue";

const props = defineProps<{
  title: string;
  confirmText: string;
  initial: string;
  submit: (name: string) => Promise<string | null>;
}>();

const emit = defineEmits<{ cancel: [] }>();
const name = ref(props.initial);
const error = ref("");
const pending = ref(false);
const input = ref<HTMLInputElement | null>(null);

onMounted(() => input.value?.focus());

async function onSubmit() {
  pending.value = true;
  error.value = "";
  const failure = await props.submit(name.value);
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
        <h2>{{ title }}</h2>
      </div>
      <div class="dialog-b">
        <label class="field-label" for="workspace-name">工作区名称</label>
        <input id="workspace-name" ref="input" v-model="name" type="text" maxlength="80" />
        <p v-if="error" class="scan-error">{{ error }}</p>
      </div>
      <div class="dialog-f">
        <button type="button" class="btn" @click="emit('cancel')">取消</button>
        <button type="submit" class="btn primary" :disabled="pending">{{ confirmText }}</button>
      </div>
    </form>
  </div>
</template>
