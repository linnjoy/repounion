<p align="center">
  <img src="src-tauri/icons/app-icon.svg" width="80" alt="RepoUnion 图标" />
</p>

# RepoUnion

**多仓库 Git 状态看板与改动审阅工具。**

管理多个项目时，不必为检查几处改动逐个打开编辑器。RepoUnion 把仓库状态、分支风险和文件差异放在一个 Windows 桌面窗口里，帮助你发现忘记提交或推送的工作，也方便核对 AI agent 修改过的项目。

[下载安装包](https://github.com/linnjoy/repounion/releases) · [反馈问题](https://github.com/linnjoy/repounion/issues) · [构建记录](https://github.com/linnjoy/repounion/actions/workflows/windows.yml)

## 能做什么

- **按工作区管理项目**：公司项目与个人项目分开加载。单独添加仓库，或扫描父目录批量导入；来源目录可自动发现新仓库，并记住手动排除的路径。
- **一眼看清状态**：当前分支、未提交改动、未跟踪项、超前与落后提交、冲突和分叉，集中展示并支持搜索、筛选。
- **检查其他分支和 worktree**：查看所有本地分支的未推送情况、stash 数量，以及其他 worktree 中遗留的改动。
- **发现新变化**：与上次确认的本地状态比较，以紫色「新变化」提示；审阅后可以手动标记已查看。
- **保护分支提醒**：在 `main`、`master`、`release/*` 等分支出现未提交改动或未推送提交时警告，规则可按工作区设置。
- **直接审阅差异**：内置文本并排 / 内联 diff、语法高亮、差异跳转，以及图片并排比较，无须先启动 IDE。
- **收工检查**：按风险汇总未提交、未推送、保护分支、worktree 和 stash 事项，帮助逐项核对。

支持跟随系统、浅色和深色主题，也可以一键打开仓库目录、终端或 VS Code。

## 下载与安装

主要支持 **Windows 10 / 11**。从 [Releases](https://github.com/linnjoy/repounion/releases) 下载 `.exe` 或 `.msi` 安装包，选择其中一种安装即可。

运行需要：

- [Git](https://git-scm.com/) **2.35 或更新**，已加入 `PATH`。
- [Microsoft Edge WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/)。

**使用安装包不需要安装 Node.js、Rust 或 Visual Studio。** 这些工具仅用于源码开发。

## 开始使用

1. 创建工作区，例如「个人项目」或「公司项目」。
2. 添加一个仓库，或选择项目父目录扫描并勾选需要管理的仓库。
3. 查看状态列表，点击仓库打开文件差异；在设置中调整保护分支和来源目录。
4. 需要了解最新远端状态时，点击「Fetch 全部」。
5. 完成审阅后「标记已查看」，离开前切到「收工检查」核对剩余事项。

### 刷新与 Fetch 的区别

| 操作 | 行为 |
| --- | --- |
| 刷新状态 | 读取本地 Git 状态、分支和 worktree 信息，不联网 |
| Fetch 全部 | 执行 `git fetch --all --prune` 更新远端引用，再读取本地状态；不合并、不改工作区文件 |
| 切回应用窗口 | 自动刷新本地状态，30 秒内不重复 |

超前 / 落后判断基于**本地保存的远端引用**，未 Fetch 时不代表远端的实时情况。结果逐仓库显示；Fetch 支持取消，单个仓库失败不会阻止其他仓库完成。

### 新变化与风险提示

「新变化」表示本地状态与上次确认时不同，不表示 RepoUnion 能判断改动来自哪个人或 agent。首次加载仓库会建立基线；之后 HEAD、本地分支、stash、改动文件或其他 worktree 的变化会参与判断。Fetch 仅更新远端引用时不触发此提示。

保护分支规则默认包含 `main`、`master`、`release/*`，`*` 匹配任意字符。这里的保护是**提醒**，不会阻止 Git 操作。

未推送统计同样依赖本地引用。没有上游的分支统计不在任何远端引用上的提交；纯本地仓库不统计，每个仓库最多对 30 个无上游分支单独计算。

### 差异查看

支持四种比较方式：工作区 vs HEAD、工作区 vs 暂存、暂存 vs HEAD、本地 vs 上游。最后一种仅在有上游且存在超前或落后提交时出现。

文件列表与内容按需读取。文本可切换并排或内联，图片可缩放或适应窗口。为避免大文件拖慢界面，文本超过约 **1.5 MB / 4 万行**、单侧图片超过约 **12 MB** 时会跳过预览；其他二进制文件显示提示。

### 常用快捷键

| 快捷键 | 功能 |
| --- | --- |
| `F5` | 刷新本地状态 |
| `Ctrl+Shift+F` | Fetch 全部 |
| `Ctrl+F` 或 `/` | 搜索仓库、分支或路径 |
| `↑` / `↓`、`Enter` | 在列表中移动、打开详情 |
| `Alt+W` | 切换全部哨兵 / 收工检查 |
| `1`～`4` | 切换可用的比较方式 |
| `[` / `]` | 上一处 / 下一处差异 |
| `Esc` | 逐层关闭浮层或详情 |

## 当前范围

RepoUnion 当前以状态检查和只读审阅为主，联网操作由用户主动触发 Fetch。暂不提供提交、暂存、推送、拉取、切换分支、合并、变基或三方冲突编辑，也不提供任意文件浏览和文件历史。

从工作区移除仓库只改变 RepoUnion 的配置，不删除磁盘上的仓库。

## 数据与配置

Windows 下配置保存在 `%APPDATA%\RepoUnion\`，不会写入被管理的 Git 仓库：

| 文件 | 内容 |
| --- | --- |
| `config.json` | 工作区、来源目录、排除清单和保护分支规则 |
| `snapshots.json` | 「新变化」的已确认状态基线 |

备份时可以复制整个目录。配置格式当前为第 2 版，旧配置会自动迁移；升级前建议备份。删除快照文件会让仓库重新建立基线。

Fetch 遇到 SSH 指纹确认或密钥口令要求时可能直接失败。请先在终端对该仓库运行一次 `git fetch`，完成主机信任或配置 SSH agent，再重试。应用默认使用非交互 SSH，但尊重已有的自定义 SSH 配置。

## 源码开发

技术栈：**Tauri 2 · Rust · Vue 3 · TypeScript · CodeMirror**。Git 操作调用本机 Git CLI，核心库独立于图形界面。

Windows 开发环境：

- Node.js 20+ 与 npm。
- [rustup](https://rustup.rs/)，工具链由 `rust-toolchain.toml` 固定。
- Visual Studio Build Tools，勾选「使用 C++ 的桌面开发」。
- Git 与 WebView2 Runtime。

```powershell
git clone https://github.com/linnjoy/repounion.git
cd repounion
npm ci
npm run tauri dev
```

`npm run dev` 仅启动前端网页，没有 Rust 后端；完整应用请使用 `npm run tauri dev`。

### 检查与构建

```powershell
npm run build
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p repounion-core
npm run tauri build
```

安装包输出到仓库根目录的 `target/release/bundle/msi/` 和 `target/release/bundle/nsis/`。

```text
core/          Git 状态、差异、扫描、配置与快照
src-tauri/     桌面入口、命令桥接与应用图标
src/           Vue 界面与前端状态
docs/          产品目标、技术路线与设计规范
```

### 自动检查与发布

| 触发方式 | 执行内容 |
| --- | --- |
| PR 创建或更新 | 前端构建、Rust 格式检查、clippy 与核心测试，不打包 |
| 仅修改 Markdown、docs/ 或 LICENSE 的 PR | 跳过检查 |
| 合并或推送 `main` | 不自动运行 |
| 推送 `v*` 标签 | 完整检查、生成安装包并发布 GitHub Release |
| 手动运行「Windows 打包」 | 完整检查并生成测试安装包 |

同一 PR 的新提交会取消旧检查。手动构建完成后，可在 Actions 运行页面的 **Artifacts → repounion-windows** 下载安装包。发布前应确认应用版本号与版本标签一致。

## 许可证

本项目采用 [MIT License](LICENSE)，Copyright © 2026 linnjoy。第三方依赖遵循各自的许可证。
