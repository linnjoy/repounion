<script setup lang="ts">
/**
 * 图片并排对比（布局规范 5.4）：缩放、适应窗口、实际大小，底部标注左右分辨率与文件大小。
 * 两侧滚动同步；只加载当前这一对图片。
 */
import { Minus, Plus } from "lucide-vue-next";
import { computed, ref } from "vue";

const props = defineProps<{
  left: string | null;
  right: string | null;
  mediaType: string;
  leftMissing: boolean;
  rightMissing: boolean;
  leftTitle: string;
  rightTitle: string;
}>();

const fit = ref(true);
const scale = ref(1);
const leftBroken = ref(false);
const rightBroken = ref(false);
const leftSize = ref({ width: 0, height: 0 });
const rightSize = ref({ width: 0, height: 0 });
const leftPane = ref<HTMLElement | null>(null);
const rightPane = ref<HTMLElement | null>(null);
let lock = false;

const leftUrl = computed(() =>
  props.left && !props.leftMissing ? `data:${props.mediaType};base64,${props.left}` : "",
);
const rightUrl = computed(() =>
  props.right && !props.rightMissing ? `data:${props.mediaType};base64,${props.right}` : "",
);
const zoomLabel = computed(() => (fit.value ? "适应" : `${Math.round(scale.value * 100)}%`));

/** base64 长度换算原始字节数：每 4 个字符 3 字节，再减去末尾填充。 */
function byteSize(base64: string | null): number {
  if (!base64) {
    return 0;
  }
  const padding = base64.endsWith("==") ? 2 : base64.endsWith("=") ? 1 : 0;
  return Math.floor((base64.length * 3) / 4) - padding;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  if (bytes < 1024 * 1024) {
    return `${(bytes / 1024).toFixed(1)} KB`;
  }
  return `${(bytes / 1024 / 1024).toFixed(2)} MB`;
}

function meta(url: string, size: { width: number; height: number }, base64: string | null): string {
  if (!url) {
    return "—";
  }
  const dims = size.width ? `${size.width}×${size.height}` : "…";
  return `${dims} (${formatBytes(byteSize(base64))})`;
}

function sized(natural: number): Record<string, string> {
  if (fit.value || natural <= 0) {
    return { maxWidth: "100%", maxHeight: "100%", objectFit: "contain" };
  }
  return { width: `${Math.round(natural * scale.value)}px`, maxWidth: "none", height: "auto" };
}

function zoomIn() {
  fit.value = false;
  scale.value = Math.min(8, Math.round(scale.value * 1.25 * 100) / 100);
}

function zoomOut() {
  fit.value = false;
  scale.value = Math.max(0.1, Math.round((scale.value / 1.25) * 100) / 100);
}

function actualSize() {
  fit.value = false;
  scale.value = 1;
}

function remember(event: Event, side: "left" | "right") {
  const image = event.target as HTMLImageElement;
  const size = { width: image.naturalWidth, height: image.naturalHeight };
  if (side === "left") {
    leftBroken.value = false;
    leftSize.value = size;
  } else {
    rightBroken.value = false;
    rightSize.value = size;
  }
}

function onScroll(event: Event) {
  if (lock) {
    return;
  }
  const source = event.target as HTMLElement;
  const other = source === leftPane.value ? rightPane.value : leftPane.value;
  if (!other) {
    return;
  }
  lock = true;
  other.scrollTop = source.scrollTop;
  other.scrollLeft = source.scrollLeft;
  lock = false;
}
</script>

<template>
  <div class="image-compare">
    <div class="diff-toolbar">
      <span class="text-muted">并排对比</span>
      <div class="diff-nav">
        <div class="zoom-group">
          <button type="button" class="btn btn-sm btn-icon" title="缩小" @click="zoomOut"><Minus :size="12" /></button>
          <span class="value">{{ zoomLabel }}</span>
          <button type="button" class="btn btn-sm btn-icon" title="放大" @click="zoomIn"><Plus :size="12" /></button>
        </div>
        <div class="mode-seg">
          <button type="button" :class="{ active: fit }" @click="fit = true">适应窗口</button>
          <button type="button" :class="{ active: !fit && scale === 1 }" @click="actualSize">实际大小 1:1</button>
        </div>
      </div>
    </div>
    <div class="img-diff-stage">
      <section class="img-diff-box">
        <header><span>左：{{ leftTitle }}</span></header>
        <div ref="leftPane" class="img-checker" @scroll="onScroll">
          <p v-if="leftMissing || !leftUrl">这一侧没有此文件</p>
          <p v-else-if="leftBroken">无法显示这张图片</p>
          <img
            v-else
            :src="leftUrl"
            alt="左侧图片"
            :style="sized(leftSize.width)"
            @load="remember($event, 'left')"
            @error="leftBroken = true"
          />
        </div>
        <div class="img-meta">{{ meta(leftUrl, leftSize, left) }}</div>
      </section>
      <section class="img-diff-box">
        <header><span>右：{{ rightTitle }}</span></header>
        <div ref="rightPane" class="img-checker" @scroll="onScroll">
          <p v-if="rightMissing || !rightUrl">这一侧没有此文件</p>
          <p v-else-if="rightBroken">无法显示这张图片</p>
          <img
            v-else
            :src="rightUrl"
            alt="右侧图片"
            :style="sized(rightSize.width)"
            @load="remember($event, 'right')"
            @error="rightBroken = true"
          />
        </div>
        <div class="img-meta">{{ meta(rightUrl, rightSize, right) }}</div>
      </section>
    </div>
  </div>
</template>
