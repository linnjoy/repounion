# RepoUnion 视觉设计规范与 CSS 变量表

> **适用范围**：Windows WebView2 (Chromium) · Vue 3 + TypeScript + 原生 CSS (`src/styles.css`)  
> **设计原则**：克制精密的桌面开发仪器感（参考 Linear / GitHub Desktop / JetBrains New UI），高信息密度但不拥挤，杜绝大圆角卡片与营销插画。

---

## 1. 视觉方向与核心决策理由

| 设计维度 | 方案决策 | 设计理由 |
| :--- | :--- | :--- |
| **层级材质** | 实色阶梯表面 + `1px` 细线分割（禁用毛玻璃大面积模糊与重投影） | 开发者每日高频切进切出，纯色明度差在 Windows 100%/125%/150% DPI 下最锐利，且在 WebView2 下滚动帧率最稳。 |
| **圆角体系** | 克制微圆角：`4px`（徽章/标签）、`6px`（按钮/输入框）、`8px`（弹窗/浮层）；**主列表行采用直角平铺** | 避免“消费级卡片感”，贴合 Windows 桌面开发工具的紧凑扫描效率。 |
| **品牌强调色** | 沿用“Union Node”熔岩橙作为品牌徽标与关键警示点，**界面常规交互主色保持克制的蓝灰/钴蓝**（亮色 `#1d4ed8` / 暗色 `#60a5fa`） | **重要权衡**：主列表中“橙色”已高频承担“有改动”语义，“红色”承担“冲突/保护分支警告”。若主交互色也用高饱和橙，会与改动状态撞色；因此保留应用图标的熔岩橙品牌基因，界面选中/焦点/主操作采用冷静的钴蓝系，使状态警示色在扫视时拥有绝对优先级。 |
| **色弱无障碍** | **严禁仅靠颜色区分状态**：左侧 `3px` 状态色条 + 独立几何图标 + 简明中文/符号文字三重冗余 | 即使在灰度模式或红绿色弱视角下，仍可通过图标形状（实心圆/三角警告/双箭头/叉号/虚线圆）和文字 1 秒判读仓库状态。 |

---

## 2. 深浅两套配色体系（完整对比度校验）

两套主题默认跟随系统 `@media (prefers-color-scheme: dark)`，并支持通过 `<html data-theme="light | dark">` 手动覆盖。所有正文与次级状态文字均满足 WCAG AA（≥ 4.5:1）。

### 2.1 表面与文字色板

| 语义 Token | 浅色主题（Alabaster Precision） | 深色主题（Obsidian Precision） | 使用场景 |
| :--- | :--- | :--- | :--- |
| `--bg-app` | `#f3f4f6` | `#0d0f12` | 最外层窗口底色、工具栏底色 |
| `--bg-sidebar` | `#eaedf1` | `#12151a` | 左侧工作区栏背景 |
| `--bg-surface` | `#ffffff` | `#161a21` | 主列表行底色、详情面板、弹窗底色 |
| `--bg-surface-subtle` | `#f8f9fb` | `#1c212b` | 来源目录分组头、表头、代码块底色 |
| `--bg-hover` | `#f1f4f9` | `#1f2531` | 列表行悬停、幽灵按钮悬停 |
| `--bg-selected` | `#e8f0fe` | `#1b283d` | 当前选中仓库行、侧边栏激活工作区 |
| `--border-subtle` | `#e4e7ec` | `#242a35` | 行间 1px 分割线、次级边框 |
| `--border-default` | `#d0d5dd` | `#303846` | 按钮、输入框、面板分割线 |
| `--text-primary` | `#111827` | `#eef2f6` | 仓库名、主标题、正文 |
| `--text-secondary` | `#4b5563` | `#9aa5b5` | 分支名、相对时间、次要说明 |
| `--text-muted` | `#6b7280` | `#6c788a` | 路径、空状态提示、快捷键提示 |

### 2.2 Git 状态语义色与无障碍冗余配对表

| 仓库/分支状态 | 浅色前景 / 浅底 | 深色前景 / 深底 | 左侧 3px 色条 | 强制配对图标 + 文案（防色盲混淆） |
| :--- | :--- | :--- | :--- | :--- |
| **有新变化 (Agent)** | `#6d28d9` / `#ede9fe` | `#c4b5fd` / `rgba(139,92,246,0.18)` | `#7c3aed`（外加左侧脉冲点） | ✦ 菱形星标 + `新变化` 胶囊 |
| **冲突 / 分叉 / 保护分支警告** | `#b91c1c` / `#fee2e2` | `#f87171` / `rgba(239,68,68,0.16)` | `#dc2626` | ▲ 实心三角警告 / ⊘ 冲突图标 + `冲突` / `分叉` / `保护分支` |
| **有未提交改动** | `#b45309` / `#fef3c7` | `#fbbf24` / `rgba(245,158,11,0.15)` | `#d97706` | ● 实心圆点 / 铅笔图标 + `3改 · 1新` |
| **干净但超前/落后** | `#1d4ed8` / `#dbeafe` | `#60a5fa` / `rgba(59,130,246,0.16)` | `#2563eb` | ↑ / ↓ 方向箭头 + `超前 2` / `落后 1` |
| **干净且同步** | `#047857` / `#d1fae5` | `#34d399` / `rgba(16,185,129,0.14)` | `#10b981` | ✓ 对勾图标 + `已同步` |
| **无上游 / 游离 HEAD** | `#4b5563` / `#f3f4f6` | `#9ca3af` / `rgba(156,163,175,0.14)` | `#9ca3af` | ○ 虚线圆 / 断链图标 + `无上游` / `游离 HEAD` |
| **读取失败 / 超时** | `#9f1239` / `#ffe4e6` | `#fb7185` / `rgba(244,63,94,0.16)` | `#e11d48`（虚线） | ✕ 警告圆环 / 沙漏图标 + `读取失败` / `超时` |

---

## 3. 字体、字号层级与间距规则

针对 Windows ClearType 渲染优化，全部数字与 Git 计数强制开启 `font-variant-numeric: tabular-nums` 确保列对齐。

### 3.1 字体栈
- **UI 正文字体**：`"Segoe UI Variable Text", "Segoe UI", "Microsoft YaHei UI", "Microsoft YaHei", sans-serif`
- **代码 / 分支 / 路径 / Diff 字体**：`"Cascadia Mono", "JetBrains Mono", Consolas, monospace`

### 3.2 字号与行高阶梯

| 级别 | Token | 字号 | 行高 | 字重 | 使用场景 |
| :--- | :--- | :--- | :--- | :--- | :--- |
| 窗口主标题 | `--fs-title` | `15px` | `20px` | `600` | 当前工作区名称、弹窗标题 |
| 列表主信息 | `--fs-body` | `13px` | `18px` | `500 / 600` | 仓库名（600）、常规按钮与表单文字（500） |
| 密集数据 / 分支 | `--fs-sm` | `12px` | `16px` | `400 / 500` | 分支名、状态文字、diff 代码行、相对时间 |
| 微标与计数 | `--fs-xs` | `11px` | `14px` | `500` | 分组标题、数字徽章、stash/worktree 胶囊、快捷键 `<kbd>` |

### 3.3 间距与尺寸基准（4px 网格）
- **基础步进**：`4px / 8px / 12px / 16px / 20px`
- **左侧工作区栏宽**：`216px`（可折叠至 `0px`，最小窗口 `1100px` 时自动保留 `196px`）
- **主区顶栏高**：`44px`；**筛选栏高**：`36px`；**来源目录分组头高**：`28px`
- **主列表单行高**：`44px`（单行主信息 + 紧凑次级路径提示，确保 1080p 屏幕一屏可展示 16~20 个仓库）
- **详情面板宽度**：默认 `560px`，支持鼠标拖拽调节，范围 **`480px ~ 1200px`**（小于 `740px` 时文件列表与 diff 自动切换为上下堆叠，大于等于 `740px` 时为左右分栏）。

---

## 4. 图标方案

### 4.1 界面图标集选型：`lucide-vue-next`（单包轻量方案）
- **选型理由**：纯 SVG 按需 Tree-shaking（最终产物仅打包用到的 ~25 个图标，增量 < 6KB），`24x24` 像素网格对齐，在 Windows 100%/125%/150% DPI 下线条不发虚。
- **统一规格**：
  - 列表状态与行内微图标：`14px × 14px`，`stroke-width="1.85"`
  - 工具栏与侧边栏图标：`16px × 16px`，`stroke-width="1.75"`
  - 空状态引导图标：`28px × 28px`，`stroke-width="1.5"`
- **核心图标映射**：
  - 分支 `GitBranch` · 保护分支警告 `ShieldAlert` · 冲突 `GitMerge` + `AlertTriangle` · 分叉 `GitFork`
  - 游离 HEAD `Unlink2` · 无上游 `CloudOff` · Stash `Archive` · Worktree `FolderGit2`
  - 新变化 `Sparkles` · 推送 `ArrowUp` · 拉取 `ArrowDown` · 刷新 `RefreshCw` · 设置 `Settings`

### 4.2 应用桌面图标（Union Node）
保留并固化「多分支汇聚核心（Union Node）」几何图标，直接用于 `src-tauri/icons/` 打包（生成 `icon.ico` 多分辨率包）：
- **大尺寸（64px / 128px / 256px）**：深炭金属超椭圆底板 + 对称汇流 Git 分支路径 + 居中熔岩橙核心节点（`#F97316`）。
- **小尺寸（16px / 32px 任务栏优化版）**：移除背景网格与辅助虚线，路径加粗至 `11.5px` 视觉等效线宽，保证 Windows 任务栏高清晰度。

---

## 5. 可直接替换至 `src/styles.css` 的 CSS 设计变量

以下变量可直接粘贴至 `src/styles.css` 顶部，同时兼容系统主题跟随与手动切换（`data-theme`）：

```css
/* ==========================================================================
   RepoUnion Design Tokens (src/styles.css)
   Environment: Windows WebView2 (Chromium) · Pure Native CSS
   ========================================================================== */

:root {
  color-scheme: light;

  /* --- Typography --- */
  --font-sans: "Segoe UI Variable Text", "Segoe UI", "Microsoft YaHei UI", "Microsoft YaHei", sans-serif;
  --font-mono: "Cascadia Mono", "JetBrains Mono", Consolas, monospace;

  --fs-title: 15px;
  --fs-body: 13px;
  --fs-sm: 12px;
  --fs-xs: 11px;

  /* --- Spacing & Layout --- */
  --space-1: 4px;
  --space-2: 8px;
  --space-3: 12px;
  --space-4: 16px;
  --space-5: 20px;

  --radius-xs: 3px;
  --radius-sm: 4px;
  --radius-md: 6px;
  --radius-lg: 8px;

  --sidebar-width: 216px;
  --topbar-height: 44px;
  --filterbar-height: 36px;
  --row-height: 44px;
  --group-header-height: 28px;

  /* --- Surfaces (Light / Alabaster Precision) --- */
  --bg-app: #f3f4f6;
  --bg-sidebar: #eaedf1;
  --bg-surface: #ffffff;
  --bg-surface-subtle: #f8f9fb;
  --bg-input: #ffffff;
  --bg-hover: #f1f4f9;
  --bg-selected: #e8f0fe;
  --bg-overlay: rgba(15, 23, 42, 0.36);

  /* --- Borders --- */
  --border-subtle: #e4e7ec;
  --border-default: #d0d5dd;
  --border-strong: #98a2b3;
  --border-focus: #2563eb;

  /* --- Text --- */
  --text-primary: #111827;
  --text-secondary: #4b5563;
  --text-muted: #6b7280;
  --text-inverse: #ffffff;

  /* --- Interactive Brand & Accent --- */
  --accent: #1d4ed8;
  --accent-hover: #1e40af;
  --accent-soft: #eff6ff;
  --accent-border: #bfdbfe;
  --brand-forge: #ea580c;

  /* --- Semantic Git Statuses --- */
  /* 1. New Changes (AI Agent Beacon) */
  --status-new-fg: #6d28d9;
  --status-new-bg: #ede9fe;
  --status-new-border: #c4b5fd;
  --status-new-bar: #7c3aed;

  /* 2. Danger: Conflict / Diverged / Protected Branch / Error */
  --status-danger-fg: #b91c1c;
  --status-danger-bg: #fee2e2;
  --status-danger-border: #fca5a5;
  --status-danger-bar: #dc2626;

  /* 3. Dirty: Uncommitted Changes */
  --status-dirty-fg: #b45309;
  --status-dirty-bg: #fef3c7;
  --status-dirty-border: #fcd34d;
  --status-dirty-bar: #d97706;

  /* 4. Sync: Clean but Ahead / Behind */
  --status-sync-fg: #1d4ed8;
  --status-sync-bg: #dbeafe;
  --status-sync-border: #93c5fd;
  --status-sync-bar: #2563eb;

  /* 5. Clean & Synced */
  --status-clean-fg: #047857;
  --status-clean-bg: #d1fae5;
  --status-clean-border: #6ee7b7;
  --status-clean-bar: #10b981;

  /* 6. Neutral: No Upstream / Detached HEAD */
  --status-neutral-fg: #4b5563;
  --status-neutral-bg: #f3f4f6;
  --status-neutral-border: #d1d5db;
  --status-neutral-bar: #9ca3af;

  /* --- Diff Syntax & Hunk Highlights --- */
  --diff-add-bg: #e6ffec;
  --diff-add-word: #abf2bc;
  --diff-add-fg: #116329;
  --diff-del-bg: #ffebe9;
  --diff-del-word: #ffcecb;
  --diff-del-fg: #a40e26;
  --diff-hunk-bg: #edf4ff;
  --diff-hunk-fg: #1d4ed8;
  --diff-gutter-bg: #f6f8fa;
  --diff-gutter-fg: #6b7280;
  --diff-active-outline: #2563eb;

  /* --- Elevation & Motion --- */
  --shadow-popover: 0 6px 20px rgba(15, 23, 42, 0.12), 0 1px 3px rgba(15, 23, 42, 0.08);
  --shadow-modal: 0 18px 48px rgba(15, 23, 42, 0.22), 0 0 0 1px rgba(15, 23, 42, 0.06);
  --ease-standard: cubic-bezier(0.2, 0, 0, 1);
  --dur-fast: 100ms;
  --dur-base: 160ms;
  --dur-panel: 200ms;
}

/* ==========================================================================
   Dark Theme (Obsidian Precision) — System Auto & Manual Override
   ========================================================================== */
@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) {
    color-scheme: dark;

    --bg-app: #0d0f12;
    --bg-sidebar: #12151a;
    --bg-surface: #161a21;
    --bg-surface-subtle: #1c212b;
    --bg-input: #101318;
    --bg-hover: #1f2531;
    --bg-selected: #1b283d;
    --bg-overlay: rgba(0, 0, 0, 0.62);

    --border-subtle: #242a35;
    --border-default: #303846;
    --border-strong: #475266;
    --border-focus: #60a5fa;

    --text-primary: #eef2f6;
    --text-secondary: #9aa5b5;
    --text-muted: #6c788a;
    --text-inverse: #0d0f12;

    --accent: #3b82f6;
    --accent-hover: #60a5fa;
    --accent-soft: rgba(59, 130, 246, 0.15);
    --accent-border: rgba(96, 165, 250, 0.35);
    --brand-forge: #f97316;

    --status-new-fg: #c4b5fd;
    --status-new-bg: rgba(139, 92, 246, 0.18);
    --status-new-border: rgba(167, 139, 250, 0.42);
    --status-new-bar: #8b5cf6;

    --status-danger-fg: #f87171;
    --status-danger-bg: rgba(239, 68, 68, 0.15);
    --status-danger-border: rgba(248, 113, 113, 0.38);
    --status-danger-bar: #ef4444;

    --status-dirty-fg: #fbbf24;
    --status-dirty-bg: rgba(245, 158, 11, 0.15);
    --status-dirty-border: rgba(251, 191, 36, 0.35);
    --status-dirty-bar: #f59e0b;

    --status-sync-fg: #60a5fa;
    --status-sync-bg: rgba(59, 130, 246, 0.16);
    --status-sync-border: rgba(96, 165, 250, 0.36);
    --status-sync-bar: #3b82f6;

    --status-clean-fg: #34d399;
    --status-clean-bg: rgba(16, 185, 129, 0.14);
    --status-clean-border: rgba(52, 211, 153, 0.32);
    --status-clean-bar: #10b981;

    --status-neutral-fg: #9ca3af;
    --status-neutral-bg: rgba(156, 163, 175, 0.13);
    --status-neutral-border: rgba(156, 163, 175, 0.28);
    --status-neutral-bar: #6b7280;

    --diff-add-bg: rgba(46, 160, 67, 0.16);
    --diff-add-word: rgba(46, 160, 67, 0.38);
    --diff-add-fg: #7ee787;
    --diff-del-bg: rgba(248, 81, 73, 0.16);
    --diff-del-word: rgba(248, 81, 73, 0.38);
    --diff-del-fg: #ffa198;
    --diff-hunk-bg: rgba(56, 139, 253, 0.14);
    --diff-hunk-fg: #79c0ff;
    --diff-gutter-bg: #12151b;
    --diff-gutter-fg: #5e6a7c;
    --diff-active-outline: #60a5fa;

    --shadow-popover: 0 8px 24px rgba(0, 0, 0, 0.5), 0 0 0 1px rgba(255, 255, 255, 0.06);
    --shadow-modal: 0 24px 60px rgba(0, 0, 0, 0.68), 0 0 0 1px rgba(255, 255, 255, 0.08);
  }
}

:root[data-theme="dark"] {
  color-scheme: dark;

  --bg-app: #0d0f12;
  --bg-sidebar: #12151a;
  --bg-surface: #161a21;
  --bg-surface-subtle: #1c212b;
  --bg-input: #101318;
  --bg-hover: #1f2531;
  --bg-selected: #1b283d;
  --bg-overlay: rgba(0, 0, 0, 0.62);

  --border-subtle: #242a35;
  --border-default: #303846;
  --border-strong: #475266;
  --border-focus: #60a5fa;

  --text-primary: #eef2f6;
  --text-secondary: #9aa5b5;
  --text-muted: #6c788a;
  --text-inverse: #0d0f12;

  --accent: #3b82f6;
  --accent-hover: #60a5fa;
  --accent-soft: rgba(59, 130, 246, 0.15);
  --accent-border: rgba(96, 165, 250, 0.35);
  --brand-forge: #f97316;

  --status-new-fg: #c4b5fd;
  --status-new-bg: rgba(139, 92, 246, 0.18);
  --status-new-border: rgba(167, 139, 250, 0.42);
  --status-new-bar: #8b5cf6;

  --status-danger-fg: #f87171;
  --status-danger-bg: rgba(239, 68, 68, 0.15);
  --status-danger-border: rgba(248, 113, 113, 0.38);
  --status-danger-bar: #ef4444;

  --status-dirty-fg: #fbbf24;
  --status-dirty-bg: rgba(245, 158, 11, 0.15);
  --status-dirty-border: rgba(251, 191, 36, 0.35);
  --status-dirty-bar: #f59e0b;

  --status-sync-fg: #60a5fa;
  --status-sync-bg: rgba(59, 130, 246, 0.16);
  --status-sync-border: rgba(96, 165, 250, 0.36);
  --status-sync-bar: #3b82f6;

  --status-clean-fg: #34d399;
  --status-clean-bg: rgba(16, 185, 129, 0.14);
  --status-clean-border: rgba(52, 211, 153, 0.32);
  --status-clean-bar: #10b981;

  --status-neutral-fg: #9ca3af;
  --status-neutral-bg: rgba(156, 163, 175, 0.13);
  --status-neutral-border: rgba(156, 163, 175, 0.28);
  --status-neutral-bar: #6b7280;

  --diff-add-bg: rgba(46, 160, 67, 0.16);
  --diff-add-word: rgba(46, 160, 67, 0.38);
  --diff-add-fg: #7ee787;
  --diff-del-bg: rgba(248, 81, 73, 0.16);
  --diff-del-word: rgba(248, 81, 73, 0.38);
  --diff-del-fg: #ffa198;
  --diff-hunk-bg: rgba(56, 139, 253, 0.14);
  --diff-hunk-fg: #79c0ff;
  --diff-gutter-bg: #12151b;
  --diff-gutter-fg: #5e6a7c;
  --diff-active-outline: #60a5fa;

  --shadow-popover: 0 8px 24px rgba(0, 0, 0, 0.5), 0 0 0 1px rgba(255, 255, 255, 0.06);
  --shadow-modal: 0 24px 60px rgba(0, 0, 0, 0.68), 0 0 0 1px rgba(255, 255, 255, 0.08);
}
```