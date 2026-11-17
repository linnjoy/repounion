# RepoUnion

RepoUnion 是一个 Windows 桌面应用，用来同时查看多个 Git 仓库的状态。它不是完整的 Git 客户端：第一版只回答「当前在哪个分支、工作区有没有改动、和远端是否一致」。

可以建立多个工作区。仓库可以一颗颗加，也可以选一个目录，递归找出里面的 Git 仓库，勾选后再加入。目录可以记为来源：下次打开工作区时自动发现新仓库，但你手动移除过的仓库不会再被加回来。

## 环境

主要目标是 Windows 10 / 11。本机需要：

- [WebView2](https://developer.microsoft.com/microsoft-edge/webview2/)（Windows 11 一般已经自带）
- [Node.js 20+](https://nodejs.org/)（自带 npm；也可以改用 pnpm）
- [rustup](https://rustup.rs/) 安装的 stable Rust（Tauri 2 需要 1.90 或更新）
- [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)，勾选「使用 C++ 的桌面开发」
- 已加入 PATH 的 [Git](https://git-scm.com/)

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

Windows 安装包在 `src-tauri/target/release/bundle/`。

## 从 Actions 下载安装包

推送到 `main`、打开指向 `main` 的拉取请求，或在 GitHub 的 Actions 页面手动运行「Windows 打包」之后，打开该次运行，在 Artifacts 里下载 `repounion-windows`。压缩包里有 WiX 的 `.msi` 和 NSIS 的安装程序 `.exe`。

推送 `v*` 标签时，同一次工作流会把这两份安装包附到对应的 GitHub Release。这个仓库不会自动创建标签或 Release。

只编译前端：

```bash
npm run build
```

只跑核心逻辑的单元测试（目录扫描和 `git status` 解析，不需要图形界面）：

```bash
cargo test -p repounion-core
```

## 配置放在哪里

工作区保存在用户目录，不写进任何 Git 仓库。

- Windows：`%APPDATA%\RepoUnion\config.json`
- Linux：`~/.config/RepoUnion/config.json`
- macOS：`~/Library/Application Support/RepoUnion/config.json`

文件里有工作区、来源目录（含是否自动扫描）、单独添加的仓库，以及每个来源的排除清单。

## 状态怎么读

- **刷新状态**只在本地执行 `git status`，不访问网络。
- **Fetch 全部**对每个仓库执行 `git fetch --all --prune`，只更新远端引用，不合并、不改工作区文件，然后再读一次本地状态。
- 查询是并行的，同时最多 8 个 Git 进程。
- 不会像编辑器那样监视父目录里的所有文件。想看最新状态，点刷新或 Fetch。

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

## 第一版不做的事

- 提交、暂存、推送、拉取、合并、变基
- diff 查看器
- 卡片视图
- 文件监视
