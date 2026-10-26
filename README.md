# GameMagNet

本地游戏目录管理器：把多个资源站点的 JSON 目录合并成一个可搜索、可去重的本地库。Rust + Tauri 2 桌面应用。

[![Release](https://img.shields.io/github/v/release/lzoreal/Game?label=release)](https://github.com/lzoreal/Game/releases/latest)
[![Build](https://github.com/lzoreal/Game/actions/workflows/build-release.yml/badge.svg)](https://github.com/lzoreal/Game/actions/workflows/build-release.yml)
[![Platform](https://img.shields.io/badge/platform-Windows-0078d4)](https://github.com/lzoreal/Game/releases/latest)
[![Rust](https://img.shields.io/badge/rust-stable-dea584)](https://www.rust-lang.org/)
[![Tauri](https://img.shields.io/badge/tauri-2-24c8db)](https://tauri.app/)

## 功能特性

- **多源合并去重**：标题归一化后合并同一游戏的不同来源，链接集中展示。
- **空搜索默认列表**：搜索框为空时显示最新的 100 个游戏，后台计算，不阻塞界面。
- **模糊搜索**：关键词模糊匹配、相似度阈值、来源筛选与日期排序。
- **详情面板**：文件大小、上传日期、来源、链接分类；复制链接、发送到 BT 客户端、查询做种热度（自动追加公共 tracker）、按需查看原始 JSON。
- **源管理**：批量添加源（一行一个直链，失败行自动跳过，结束后统一汇总）、从本地导入 JSON、更新选中源、最多 4 路并行刷新全部源、删除源。
- **Cloudflare 自动验证**：内置 chaser-cf，遇到人机验证时自动启动本机 Chrome/Edge 完成验证并重试；失败时仍提供「用浏览器打开」与本地导入的引导。
- **应用内更新**：发现新版本后可下载 release 中的单文件 exe，校验 SHA-256 后自动备份、覆盖当前目录并重启；目录不可写时会提示改用安装包。
- **明暗主题**：手动切换并记住选择，原生标题栏同步切换。
- **快捷键**：`Ctrl+U` 添加源、`Ctrl+I` 导入 JSON、`F5` 刷新全部源、`Ctrl+K` / `Ctrl+F` 聚焦搜索、`Ctrl+B` 资料库、`Ctrl+D` 详情。

## 下载

前往 [Releases](https://github.com/lzoreal/Game/releases/latest) 下载：

| 文件 | 说明 |
| --- | --- |
| `GameMagNet-windows.zip` | 便携目录版（推荐），解压后运行 `GameMagNet/GameMagNet.exe` |
| `GameMagNet.exe` | 单文件免安装版，与 `sources/` 同级存放源文件 |
| `GameMagNet_*_x64-setup.exe` | NSIS 安装包（由 CI 生成） |

可执行文件与 `sources/` 目录同级存放源文件，升级时保留该目录即可。应用依赖系统 WebView2（Windows 11 已预装；Windows 10 需要安装 Edge WebView2 Runtime）。

## 使用

1. 首次启动会自动创建 `sources/` 目录。
2. 通过「添加源」粘贴一个或多个目录 JSON 直链（一行一个），或「从本地导入 JSON」导入已有文件。批量添加时空行与重复地址会被忽略，失败行跳过并继续处理，全部结束后显示成功/失败汇总。
3. 搜索框为空时浏览最新游戏；输入关键词后按防抖自动搜索。
4. 单击结果在详情面板查看，双击直接打开详情；磁力链接可发送到本机 BT 客户端或查询热度。
5. 「更新选中源」和「刷新全部源」会重新下载源 JSON，并记录源地址用于下次更新；刷新最多 4 路并发，慢速网络上的大文件超时会列入失败汇总。
6. 「检查更新」发现新版本后，点「下载并安装」会自动替换当前目录的程序并重启；只读目录会提示改用安装包或发布页。
7. 侧栏可用顶栏的面板按钮或 `Ctrl+B` 折叠/关闭。

### Cloudflare 站点

部分源站点启用了 Cloudflare。应用会先带浏览器风格请求头与三种浏览器指纹（Chrome / Safari / Firefox）直接请求；仍被拦截时，使用内嵌的 [chaser-cf](https://crates.io/crates/chaser-cf) 自动验证：

- 自动启动本机 Chrome 或 Edge（有可见窗口）完成托管质询，验证通过后直接由浏览器读取源 JSON，避免 HTTP 客户端与浏览器指纹不一致导致二次拦截。
- 无感质询直接通过；需要点击的 Turnstile 会通过 CDP 穿透 shadow root / iframe 自动点击。
- 验证并读取完成后自动关闭验证浏览器窗口；同一 profile 会复用已有 clearance，下次只需重新打开浏览器。
- 每次启动验证浏览器时使用独立的临时浏览器 profile，退出时尝试清理，并在下次启动时清理过期目录；避免残留进程占用固定目录导致初始化失败。
- 浏览器首次遇到验证时才启动，不会拖慢应用启动；多个源共享同一浏览器实例。
- 单次验证可能需要 30–90 秒；默认超时 120 秒，可用 `CHASER_TIMEOUT`（毫秒）调整。
- 需要本机安装 Chrome 或 Edge（Windows 会自动探测，Edge 回退可用）；可用 `CHROME_BIN` 指定浏览器路径，`CHASER_EXTRA_ARGS` 追加 Chrome 参数。
- 托管质询在无头模式下通常无法通过，因此默认使用可见窗口（`CHASER_HEADLESS` 仅建议用于 Turnstile 场景）。
- 如需完全关闭自动验证，设置环境变量 `GAMEMAGNET_CHASER=0`。
- 并发上限默认 4 个浏览器上下文，可用 `CHASER_CONTEXT_LIMIT` 调整。

自动验证失败时，应用仍会提示源地址，可以：

1. 在浏览器中打开该地址完成验证，保存返回的 JSON，再通过「从本地导入 JSON」导入；
2. 改用不受保护的镜像直链；
3. 改用在浏览器中可正常访问的其他源。

内嵌 chaser-cf 会增大安装包体积，但浏览器进程只在验证时启动。

## 从源码运行

前置：Rust stable-msvc、VS Build Tools 2022（C++ 工作负载）、Node.js 20+。

```bash
npm install
npm run dev
```

## 构建

```bash
npm run build
```

产物：

- `src-tauri/target/release/gamemagnet.exe`（单文件可执行）
- `src-tauri/target/release/bundle/nsis/GameMagNet_*_x64-setup.exe`（安装包；构建时需要访问 GitHub 下载 NSIS）

便携包（exe + 空 `sources/`）：

```powershell
New-Item -ItemType Directory -Force dist/GameMagNet/sources | Out-Null
Copy-Item src-tauri/target/release/gamemagnet.exe dist/GameMagNet/GameMagNet.exe
Compress-Archive -Path dist/GameMagNet/* -DestinationPath dist/GameMagNet-windows.zip -Force
```

发布新版本时同步版本号（`tauri.conf.json`、`Cargo.toml`、`APP_VERSION`）：

```bash
node scripts/sync-version.mjs v1.2.0
```

如果所在网络无法直连 crates.io，可在用户级 `~/.cargo/config.toml` 自行配置 sparse 镜像。

调试更新流程时可用 `GAMEMAGNET_UPDATE_API` 指向一个兼容 GitHub Releases API 的地址（默认 `https://api.github.com`）。

## 测试

```bash
cargo test --lib --manifest-path src-tauri/Cargo.toml
```

70 个 Rust 测试覆盖目录解析与去重、搜索与排序、gzip 存储、源命令、批量添加、详情模型、Tracker 热度、BT 交接、更新检查、Cloudflare 兜底与应用状态流程。

## 性能

本机基准（245,302 条目录 / 208,091 组，Windows 11）：

- 启动到可用：约 1.4 秒
- 私有内存：约 265MB，加载后稳定（瞬时峰值约 420MB）
- 空闲 CPU：低于单核 1%
- 搜索 `hades`：约 0.02 秒

目录解析采用零拷贝（直接移动 JSON 字符串）、64 位稳定去重键、来源名共享，并按 CPU 并行加载各源（最多 4 线程，可用环境变量 `GAMEMAGNET_LOAD_WORKERS` 调整）。标题归一化与去重键共享同一次 Unicode 规范化。

## 目录结构

| 路径 | 说明 |
| --- | --- |
| `src-tauri/src/catalog.rs` | 目录解析、去重键、搜索、最新列表 |
| `src-tauri/src/storage.rs` | gzip 读写、原子写、源地址 sidecar |
| `src-tauri/src/commands.rs` | 源目录写入：下载、添加、更新、导入、删除、刷新 |
| `src-tauri/src/detail.rs` | 详情卡片与链接分组 |
| `src-tauri/src/chaser.rs` | 内嵌 chaser-cf Cloudflare 自动验证与 cookie 提取 |
| `src-tauri/vendor/chaser-cf/` | chaser-cf 0.2.1 本地副本（补丁：支持独立浏览器 profile） |
| `src-tauri/src/swarm.rs` | bencode 解析与 HTTP/UDP tracker 热度查询 |
| `src-tauri/src/bt.rs` | BT 客户端探测与交接计划 |
| `src-tauri/src/update.rs` | GitHub Release 更新检查 |
| `src-tauri/src/state.rs` | 应用状态、任务调度与事件 |
| `src-tauri/src/ipc.rs` | 前端 IPC 命令 |
| `ui/` | 前端界面（HTML/CSS/JS） |
| `scripts/sync-version.mjs` | 发布时同步版本号 |
| `sources/` | 本地源目录（用户数据，不提交） |
| `.github/workflows/build-release.yml` | CI：测试、打包、发布 |

## 数据与隐私

`sources/` 属于本地数据，不随仓库提交；应用只在本地读取这些 JSON，不收集或上传任何信息。磁力热度查询只向磁力链接内的 tracker 发送 announce 请求，不下载种子或游戏内容。

## 贡献

欢迎提交 Issue 和 Pull Request：

1. Fork 本仓库并从 `master` 创建分支，例如 `feat/source-filter`、`fix/cf-retry`。
2. 保持改动聚焦，遵循现有代码风格与中文界面文案。
3. 新增或修改逻辑时补充 Rust 测试，并确保全量测试通过。
4. 在 PR 描述中说明动机、改动点和验证方式。

Bug 报告请尽量包含：Windows 版本、应用版本、重现步骤、错误提示或日志。

## 贡献者

感谢所有提交代码、反馈问题、改进文档的人。

<a href="https://github.com/lzoreal/Game/graphs/contributors">
  <img src="https://contrib.rocks/image?repo=lzoreal/Game" alt="Contributors" />
</a>

## 免责声明

本项目仅用于管理和检索公开的游戏目录 JSON 与磁力链接，不托管、不存储、不分发任何游戏文件。请遵守所在地区的法律法规以及各资源站点的使用条款。
