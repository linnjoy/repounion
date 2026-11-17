<script setup lang="ts">
defineProps<{
  title: string;
  body: string;
  confirmText: string;
  danger?: boolean;
}>();

const emit = defineEmits<{ cancel: []; confirm: [] }>();
</script>

<template>
  <div class="overlay" @mousedown.self="emit('cancel')">
    <div class="dialog sm" role="dialog" aria-modal="true">
      <div class="dialog-h">
        <h2>{{ title }}</h2>
        <p>{{ body }}</p>
      </div>
      <div class="dialog-f">
        <button type="button" class="btn" @click="emit('cancel')">取消</button>
        <button type="button" class="btn" :class="danger ? 'danger' : 'primary'" @click="emit('confirm')">
          {{ confirmText }}
        </button>
      </div>
    </div>
  </div>
</template>
