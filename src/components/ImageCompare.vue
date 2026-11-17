<script setup lang="ts">
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
const leftWidth = ref(0);
const rightWidth = ref(0);
const leftPane = ref<HTMLElement | null>(null);
const rightPane = ref<HTMLElement | null>(null);
let lock = false;

const leftUrl = computed(() =>
  props.left && !props.leftMissing ? `data:${props.mediaType};base64,${props.left}` : "",
);
const rightUrl = computed(() =>
  props.right && !props.rightMissing ? `data:${props.mediaType};base64,${props.right}` : "",
);
const zoomLabel = computed(() => (fit.value ? "适应窗口" : `${Math.round(scale.value * 100)}%`));

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

function fitWindow() {
  fit.value = true;
}

function actualSize() {
  fit.value = false;
  scale.value = 1;
}

function rememberWidth(event: Event, side: "left" | "right") {
  const image = event.target as HTMLImageElement;
  if (side === "left") {
    leftBroken.value = false;
    leftWidth.value = image.naturalWidth;
  } else {
    rightBroken.value = false;
    rightWidth.value = image.naturalWidth;
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
    <div class="image-tools">
      <button type="button" @click="zoomOut">缩小</button>
      <button type="button" @click="zoomIn">放大</button>
      <button type="button" :class="{ on: fit }" @click="fitWindow">适应窗口</button>
      <button type="button" :class="{ on: !fit && scale === 1 }" @click="actualSize">实际大小</button>
      <span>{{ zoomLabel }}</span>
    </div>
    <div class="image-pair">
      <section>
        <header>{{ leftTitle }}</header>
        <div ref="leftPane" class="image-pane" @scroll="onScroll">
          <p v-if="leftMissing || !leftUrl">这一侧没有此文件</p>
          <p v-else-if="leftBroken">无法显示这张图片</p>
          <img
            v-else
            :src="leftUrl"
            alt="左侧图片"
            :style="sized(leftWidth)"
            @load="rememberWidth($event, 'left')"
            @error="leftBroken = true"
          />
        </div>
      </section>
      <section>
        <header>{{ rightTitle }}</header>
        <div ref="rightPane" class="image-pane" @scroll="onScroll">
          <p v-if="rightMissing || !rightUrl">这一侧没有此文件</p>
          <p v-else-if="rightBroken">无法显示这张图片</p>
          <img
            v-else
            :src="rightUrl"
            alt="右侧图片"
            :style="sized(rightWidth)"
            @load="rememberWidth($event, 'right')"
            @error="rightBroken = true"
          />
        </div>
      </section>
    </div>
  </div>
</template>
