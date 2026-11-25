<script setup lang="ts">
/**
 * RepoUnion「Union Node」应用图标（视觉规范 4.2）：两条分支汇入熔岩橙核心。
 * 采用小尺寸优化版几何（无背景网格、路径加粗），深炭机壳在浅深两套主题下保持不变。
 */
withDefaults(defineProps<{ size?: number }>(), { size: 20 });
</script>

<template>
  <svg :width="size" :height="size" viewBox="0 0 128 128" fill="none" aria-hidden="true">
    <rect x="4" y="4" width="120" height="120" rx="28" fill="#181C24" stroke="#363E4D" stroke-width="4" />
    <path
      d="M36 92V68C36 53 47 46 60 54L64 57M92 36V60C92 75 81 82 68 74L64 71"
      stroke="#F8FAFC"
      stroke-width="11"
      stroke-linecap="round"
    />
    <circle cx="36" cy="92" r="8" fill="#181C24" stroke="#F8FAFC" stroke-width="4.5" />
    <circle cx="92" cy="36" r="8" fill="#181C24" stroke="#F8FAFC" stroke-width="4.5" />
    <circle cx="64" cy="64" r="19" fill="#12151C" stroke="#475569" stroke-width="3" />
    <circle cx="64" cy="64" r="11.5" fill="#F97316" />
  </svg>
</template>
