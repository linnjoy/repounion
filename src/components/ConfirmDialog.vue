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
  <div class="modal-backdrop" @mousedown.self="emit('cancel')">
    <div class="modal-dialog sm" role="alertdialog" aria-modal="true">
      <div class="modal-head">
        <h3>{{ title }}</h3>
      </div>
      <div class="modal-body">
        <p>{{ body }}</p>
      </div>
      <div class="modal-foot">
        <div class="right">
          <button type="button" class="btn" @click="emit('cancel')">取消</button>
          <button type="button" class="btn" :class="danger ? 'btn-danger' : 'btn-primary'" @click="emit('confirm')">
            {{ confirmText }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
