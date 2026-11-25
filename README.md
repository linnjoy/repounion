# RepoUnion

RepoUnion 是一个 Windows 桌面应用，用来同时查看多个 Git 仓库的状态。它不是完整的 Git 客户端：先回答「当前在哪个分支、工作区有没有改动、和远端是否一致」，点开某个仓库后可以再看改动文件和文本或图片差异。

可以建立多个工作区。仓库可以一颗颗加，也可以选一个目录，递归找出里面的 Git 仓库，勾选后再加入。目录可以记为来源：下次打开工作区时自动发现新仓库，但你手动移除过的仓库不会再被加回来。

## 环境

主要目标是 Windows 10 / 11。本机需要：

- [WebView2](https://developer.microsoft.com/microsoft-edge/webview2/)（Windows 11 一般已经自带）
- [Node.js 20+](https://nodejs.org/)（自带 npm；也可以改用 pnpm）
- [rustup](https://rustup.rs/)。具体 Rust 版本由仓库根的 `rust-toolchain.toml` 固定，进入仓库后 rustup 会自动使用
- [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)，勾选「使用 C++ 的桌面开发」
- 已加入 PATH 的 [Git](https://git-scm.com/) 2.35 或更新

Linux 上开发还需要 WebKitGTK，例如 Ubuntu：

```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev patchelf build-essential pkg-config libssl-dev
```

## 开发

在仓库根目录：

```bash
npm install
npm run tauri dev
```

使用 pnpm 时：

```bash
pnpm install
pnpm tauri dev
```

`npm run dev` 只会启动网页，没有 Rust 后端，添加仓库和读状态都不可用。请用 `npm run tauri dev`。

## 打包

```bash
npm run tauri build
```

Windows 安装包在仓库根的 `target/release/bundle/`（`msi/` 和 `nsis/` 两个子目录）。
根目录 `Cargo.toml` 是 Cargo 工作区，编译产物统一放在仓库根的 `target`，不在 `src-tauri/target`。

Rust 版本由仓库根的 `rust-toolchain.toml` 固定，rustup 会自动切换到该版本。

## 从 Actions 下载安装包

推送到 `main`、打开指向 `main` 的拉取请求，或在 GitHub 的 Actions 页面手动运行「Windows 打包」之后，打开该次运行，在 Artifacts 里下载 `repounion-windows`。压缩包里有 WiX 的 `.msi` 和 NSIS 的安装程序 `.exe`。

推送 `v*` 标签时，同一次工作流会把这两份安装包附到对应的 GitHub Release。这个仓库不会自动创建标签或 Release。

只编译前端：

```bash
npm run build
```

只跑核心逻辑的单元测试（目录扫描、`git status` 解析、改动文件列表和大文件限制，不需要图形界面）：

```bash
cargo test -p repounion-core
```

## 配置放在哪里

工作区保存在用户目录，不写进任何 Git 仓库。

- Windows：`%APPDATA%\RepoUnion\config.json`
- Linux：`~/.config/RepoUnion/config.json`
- macOS：`~/Library/Application Support/RepoUnion/config.json`

文件里有工作区、来源目录（含是否自动扫描）、单独添加的仓库、每个来源的排除清单，以及每个工作区的保护分支规则。
配置格式当前为第 2 版；旧版配置打开时会自动补上默认保护分支，下次保存时升级。更新版本程序写入的配置，旧程序会拒绝打开，避免丢字段。

## 状态怎么读

- **刷新状态**只在本地执行 `git status`，不访问网络。
- **Fetch 全部**对每个仓库执行 `git fetch --all --prune`，只更新远端引用，不合并、不改工作区文件，然后再读一次本地状态。
- 查询是并行的，同时最多 8 个 Git 进程。每读完一个仓库就更新对应的一行，不用等全部完成。
- 每个 Git 进程都有超时：本地命令 30 秒，fetch 120 秒。超时会终止该仓库的进程，不影响其他仓库。
- Fetch 进行中可以点「取消 Fetch」：正在运行的进程会被终止，没开始的仓库跳过，本地状态照常重新读取。
- Fetch 时如果你没有自己配置 SSH 程序（`GIT_SSH_COMMAND`、`GIT_SSH` 或 `core.sshCommand`），
  会使用 `ssh -o BatchMode=yes`：需要确认主机指纹或输入密钥口令的仓库直接报错，不会在后台卡住。
  遇到这种情况，先在终端里对该仓库手动执行一次 `git fetch`。
- 窗口重新获得焦点时会自动刷新一次本地状态（不联网），30 秒内不重复。
- 不会像编辑器那样监视父目录里的所有文件。
- 在 Windows 上启动 Git 时带 `CREATE_NO_WINDOW`，刷新和 Fetch 不会再闪出命令行窗口。

扫描目录时查找 `.git` 目录或 `gitdir:` 文件。会跳过 `node_modules`、`.git` 内部，以及 `target`、`dist`、`build` 这类常见构建产物目录里的嵌套仓库。

列表左侧色条：

| 颜色 | 含义 |
| --- | --- |
| 橙 | 有本地改动 |
| 蓝 | 工作区干净，但超前或落后远端 |
| 红 | 有冲突，或与远端分叉 |
| 绿 | 干净且已与远端一致 |
| 灰 | 干净，但没有上游分支 |

冲突或分叉优先于「有改动」。有改动优先于「没有上游」。筛选里的「有改动」「未同步」「冲突」和顶部汇总用的是同一套优先级；「无上游」可以和「有改动」同时成立。

## 看清还没收尾的地方

每次刷新除了当前分支，还会读取下面这些本地信息（都不联网）：

- **所有本地分支的未推送提交**：有上游的分支看超前数；没有上游或上游已删除的分支，统计不在任何远端引用上的提交数。
  纯本地仓库（没有任何远端）不统计。每个仓库最多对 30 个无上游分支单独统计。
- **stash 数量**：使用 `git status --show-stash`，需要 Git 2.35 或更新。
- **其他 worktree**：同一仓库的 linked worktree 及其中未提交的文件数，方便发现 agent 在 worktree 里留下的改动。
- **保护分支**：每个工作区可在左侧工作区菜单「保护分支…」里设置规则，默认 `main`、`master`、`release/*`，
  `*` 匹配任意字符。当前分支命中规则且有未提交改动或未推送提交时，行内显示「⚠ 保护分支」。
- **有新变化**：记住你上次「标记已查看」时每个仓库的状态，之后 HEAD、任何本地分支、stash、改动文件
  （包括已改文件再次被修改）或其他 worktree 的改动发生变化，就显示「● 有新变化」，直到你再次标记。
  第一次见到的仓库以当时状态为基准，不会提示。Fetch 只改远端引用，不会触发这个提示。

顶部「收工检查」按仓库、分支列出所有还没收尾的事项：未提交改动、未推送提交、stash、保护分支上的改动、
有改动的其他 worktree。筛选条的「有新变化」「未收尾」也可以只看这些仓库。

「有新变化」的基准保存在 `%APPDATA%\RepoUnion\snapshots.json`，和工作区配置分开。删除它只会让所有仓库重新建立基准。

## 查看改动

点仓库列表里的一行，右侧打开详情，可以拖动分隔条调整宽度。文件列表和文件内容都是点开之后才读取的，不会预先加载整个工作区。

文件按暂存、未暂存、未跟踪和冲突分组，显示路径和状态。比较方式可以切换：

- 工作区 vs HEAD
- 工作区 vs 暂存
- 暂存 vs HEAD
- 本地 vs 上游：仅当这个仓库有上游，并且超前或落后时出现。只读本地已经有的引用，不会自己去 fetch。想更新引用，先点「Fetch 全部」。

文本文件可以在并排和内联之间切换，带语法高亮、上一处 / 下一处，以及「3 / 12 处差异」这样的计数。并排时两侧按差异对齐，一起滚动。详情栏里只有一个编辑器，换文件时更新内容，不会每个文件各开一个。

图片（png、jpg、jpeg、gif、webp、bmp、svg 等）并排显示，可以放大、缩小、适应窗口或看实际大小，并且只加载当前这一对。其它二进制文件会提示无法预览，不会当成文本打开。

文本超过约 1.5 MB 或约 4 万行、单侧图片超过约 12 MB 时，会提示并跳过内容。点「刷新状态」或「Fetch 全部」之后，已经打开的详情会重新读取该仓库的文件列表。

差异分段在 Rust 侧用 `similar` 计算，界面用 CodeMirror 的并排 / 内联视图显示。仍然调用本机 `git`，没有换成 libgit2。

## 还没做的事

- 文件检视：任意浏览仓库里的文件，而不只是当前改动
- 提交、暂存、推送、拉取、合并、变基
- 三方冲突编辑
- 任意两个文件或目录的对比、分支对比工具、文件历史
- 卡片视图
- 文件监视
