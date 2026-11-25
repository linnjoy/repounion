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
  <div class="modal-backdrop" @mousedown.self="emit('cancel')">
    <form class="modal-dialog sm" role="dialog" aria-modal="true" @submit.prevent="onSubmit">
      <div class="modal-head">
        <h3>{{ title }}</h3>
      </div>
      <div class="modal-body">
        <div>
          <label class="field-label" for="workspace-name">工作区名称</label>
          <input
            id="workspace-name"
            ref="input"
            v-model="name"
            class="text-input"
            type="text"
            maxlength="80"
            placeholder="例如：个人项目、公司项目"
          />
        </div>
        <p v-if="error" class="form-error">{{ error }}</p>
      </div>
      <div class="modal-foot">
        <div class="right">
          <button type="button" class="btn" @click="emit('cancel')">取消</button>
          <button type="submit" class="btn btn-primary" :disabled="pending">{{ confirmText }}</button>
        </div>
      </div>
    </form>
  </div>
</template>
