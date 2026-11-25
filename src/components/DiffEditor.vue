<script setup lang="ts">
import { javascript } from "@codemirror/lang-javascript";
import { json } from "@codemirror/lang-json";
import { html } from "@codemirror/lang-html";
import { css } from "@codemirror/lang-css";
import { rust } from "@codemirror/lang-rust";
import { python } from "@codemirror/lang-python";
import { markdown } from "@codemirror/lang-markdown";
import { xml } from "@codemirror/lang-xml";
import { cpp } from "@codemirror/lang-cpp";
import { java } from "@codemirror/lang-java";
import { go } from "@codemirror/lang-go";
import { sql } from "@codemirror/lang-sql";
import { php } from "@codemirror/lang-php";
import { defaultHighlightStyle, StreamLanguage, syntaxHighlighting } from "@codemirror/language";
import { dockerFile } from "@codemirror/legacy-modes/mode/dockerfile";
import { powerShell } from "@codemirror/legacy-modes/mode/powershell";
import { properties } from "@codemirror/legacy-modes/mode/properties";
import { ruby } from "@codemirror/legacy-modes/mode/ruby";
import { shell } from "@codemirror/legacy-modes/mode/shell";
import { toml } from "@codemirror/legacy-modes/mode/toml";
import { yaml } from "@codemirror/legacy-modes/mode/yaml";
import { Change, MergeView, getChunks, getOriginalDoc, originalDocChangeEffect, unifiedMergeView } from "@codemirror/merge";
import { ChangeSet, Compartment, EditorState, type Extension } from "@codemirror/state";
import { EditorView, lineNumbers } from "@codemirror/view";
import { onBeforeUnmount, onMounted, ref, watch } from "vue";
import type { DiffLayout, HunkSpan } from "../types";

const props = defineProps<{
  left: string;
  right: string;
  filename: string;
  layout: DiffLayout;
  hunks: HunkSpan[];
}>();

const emit = defineEmits<{
  chunks: [count: number];
}>();

const host = ref<HTMLElement | null>(null);
const hunkRef = { current: [] as HunkSpan[] };

let mergeView: MergeView | null = null;
let unifiedView: EditorView | null = null;
let langA = new Compartment();
let langB = new Compartment();
let langU = new Compartment();
let alive = false;

function languageOf(filename: string): Extension {
  const base = filename.split(/[/\\]/).pop() ?? filename;
  const lower = base.toLowerCase();
  if (lower === "dockerfile" || lower.startsWith("dockerfile.")) {
    return StreamLanguage.define(dockerFile);
  }
  const ext = lower.includes(".") ? lower.slice(lower.lastIndexOf(".") + 1) : "";
  switch (ext) {
    case "js":
    case "mjs":
    case "cjs":
      return javascript();
    case "jsx":
      return javascript({ jsx: true });
    case "ts":
    case "mts":
    case "cts":
      return javascript({ typescript: true });
    case "tsx":
      return javascript({ jsx: true, typescript: true });
    case "json":
      return json();
    case "html":
    case "htm":
    case "vue":
      return html();
    case "css":
    case "scss":
    case "less":
      return css();
    case "rs":
      return rust();
    case "py":
      return python();
    case "md":
    case "markdown":
      return markdown();
    case "xml":
    case "svg":
      return xml();
    case "c":
    case "h":
    case "cpp":
    case "cc":
    case "cxx":
    case "hpp":
      return cpp();
    case "java":
      return java();
    case "go":
      return go();
    case "sql":
      return sql();
    case "php":
      return php();
    case "sh":
    case "bash":
    case "zsh":
      return StreamLanguage.define(shell);
    case "yml":
    case "yaml":
      return StreamLanguage.define(yaml);
    case "toml":
      return StreamLanguage.define(toml);
    case "rb":
      return StreamLanguage.define(ruby);
    case "ps1":
      return StreamLanguage.define(powerShell);
    case "ini":
    case "cfg":
    case "properties":
      return StreamLanguage.define(properties);
    default:
      return [];
  }
}

function editorExtensions(language: Compartment, filename: string): Extension[] {
  return [
    lineNumbers(),
    EditorState.readOnly.of(true),
    EditorState.tabSize.of(4),
    EditorView.editable.of(false),
    syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
    language.of(languageOf(filename)),
    EditorView.theme({
      "&": { fontSize: "12.5px" },
      ".cm-scroller": { fontFamily: "var(--mono)" },
      ".cm-gutters": {
        backgroundColor: "#f7f8fa",
        color: "#98a2b3",
        borderRight: "1px solid #e4e7ec",
      },
      ".cm-activeLineGutter": { backgroundColor: "transparent" },
    }),
  ];
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

function toChanges(left: string, right: string, hunks: HunkSpan[]): Change[] {
  const changes: Change[] = [];
  for (const hunk of hunks) {
    const fromA = clamp(hunk.fromA, 0, left.length);
    const toA = clamp(hunk.toA, fromA, left.length);
    const fromB = clamp(hunk.fromB, 0, right.length);
    const toB = clamp(hunk.toB, fromB, right.length);
    if (fromA === toA && fromB === toB) {
      continue;
    }
    changes.push(new Change(fromA, toA, fromB, toB));
  }
  return changes;
}

function replaceDoc(view: EditorView, text: string) {
  if (view.state.doc.toString() === text) {
    return;
  }
  view.dispatch({
    changes: { from: 0, to: view.state.doc.length, insert: text },
  });
}

function destroyViews() {
  mergeView?.destroy();
  mergeView = null;
  unifiedView?.destroy();
  unifiedView = null;
  langA = new Compartment();
  langB = new Compartment();
  langU = new Compartment();
  if (host.value) {
    host.value.replaceChildren();
  }
}

function createSide() {
  if (!host.value) {
    return;
  }
  mergeView = new MergeView({
    a: { doc: props.left, extensions: editorExtensions(langA, props.filename) },
    b: { doc: props.right, extensions: editorExtensions(langB, props.filename) },
    parent: host.value,
    highlightChanges: true,
    gutter: true,
    diffConfig: {
      override: (left, right) => toChanges(left, right, hunkRef.current),
    },
  });
}

function createInline() {
  if (!host.value) {
    return;
  }
  unifiedView = new EditorView({
    doc: props.right,
    parent: host.value,
    extensions: [
      ...editorExtensions(langU, props.filename),
      unifiedMergeView({
        original: props.left,
        mergeControls: false,
        highlightChanges: true,
        gutter: true,
        diffConfig: {
          override: (left, right) => toChanges(left, right, hunkRef.current),
        },
      }),
    ],
  });
}

function updateInline() {
  if (!unifiedView) {
    return;
  }
  const original = getOriginalDoc(unifiedView.state);
  if (original.toString() !== props.left) {
    const changes = ChangeSet.of([{ from: 0, to: original.length, insert: props.left }], original.length);
    unifiedView.dispatch({
      effects: originalDocChangeEffect(unifiedView.state, changes),
    });
  }
  replaceDoc(unifiedView, props.right);
  const next = languageOf(props.filename);
  unifiedView.dispatch({ effects: langU.reconfigure(next) });
}

function updateSide() {
  if (!mergeView) {
    return;
  }
  const nextA = languageOf(props.filename);
  const nextB = languageOf(props.filename);
  mergeView.a.dispatch({ effects: langA.reconfigure(nextA) });
  mergeView.b.dispatch({ effects: langB.reconfigure(nextB) });
  replaceDoc(mergeView.a, props.left);
  replaceDoc(mergeView.b, props.right);
}

function chunkList() {
  if (mergeView) {
    return mergeView.chunks;
  }
  if (unifiedView) {
    return getChunks(unifiedView.state)?.chunks ?? [];
  }
  return [];
}

function publishChunks() {
  emit("chunks", chunkList().length);
}

function sync(forceRebuild: boolean) {
  hunkRef.current = props.hunks;
  const wantSide = props.layout === "side";
  const hasSide = mergeView !== null;
  const hasInline = unifiedView !== null;
  if (forceRebuild || (wantSide && !hasSide) || (!wantSide && !hasInline)) {
    destroyViews();
    if (wantSide) {
      createSide();
    } else {
      createInline();
    }
  } else if (wantSide) {
    updateSide();
  } else {
    updateInline();
  }
  publishChunks();
}

function scrollTo(index: number) {
  const chunks = chunkList();
  const chunk = chunks[index];
  if (!chunk) {
    return;
  }
  if (mergeView) {
    // 并排视图的外层一起滚动，两侧按差异对齐，所以只滚这一处。
    const pos = Math.min(chunk.endB, mergeView.b.state.doc.length);
    const top = mergeView.b.lineBlockAt(pos).top;
    mergeView.dom.scrollTop = Math.max(0, top - mergeView.dom.clientHeight * 0.3);
    return;
  }
  if (unifiedView) {
    const pos = Math.min(chunk.endB, unifiedView.state.doc.length);
    unifiedView.dispatch({
      effects: EditorView.scrollIntoView(pos, { y: "center" }),
    });
  }
}

defineExpose({ scrollTo });

onMounted(() => {
  alive = true;
  sync(true);
});

watch(
  () => [props.left, props.right, props.filename, props.layout, props.hunks] as const,
  (next, prev) => {
    if (!alive) {
      return;
    }
    const layoutChanged = !prev || next[3] !== prev[3];
    sync(layoutChanged);
  },
);

onBeforeUnmount(() => {
  alive = false;
  destroyViews();
});
</script>

<template>
  <!-- 详情栏全程只挂这一处编辑器：换文件时改文档，并排和内联来回切时先销毁再创建另一个。 -->
  <div ref="host" class="diff-host"></div>
</template>
