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
- **源管理**：批量添加源（一行一个直链，失败行自动跳过，结束后统一汇总）、从本地导入 JSON、更新选中源、刷新全部源、删除源。
- **Cloudflare 处理**：识别人机验证页面，提供「用浏览器打开」与本地导入的引导。
- **检查更新**：从 GitHub Release 获取新版本信息。
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
5. 「更新选中源」和「刷新全部源」会重新下载源 JSON，并记录源地址用于下次更新。
6. 侧栏可用顶栏的面板按钮或 `Ctrl+B` 折叠/关闭。

### Cloudflare 站点

部分源站点启用了 Cloudflare。应用使用系统 TLS 发起请求，并识别人机验证页面：

- 常规请求会带上浏览器风格的请求头。
- 需要浏览器执行 JavaScript 的托管质询（页面显示 “Just a moment...” / “请稍候…”）无法自动绕过。应用会识别并在对话框中提供「用浏览器打开」按钮与源地址。
- 遇到这种站点时，可以：
  1. 在浏览器中打开该地址完成验证，保存返回的 JSON，再通过「从本地导入 JSON」导入；
  2. 改用不受保护的镜像直链；
  3. 改用在浏览器中可正常访问的其他源。

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

## 测试

```bash
cargo test --lib --manifest-path src-tauri/Cargo.toml
```

57 个 Rust 测试覆盖目录解析与去重、搜索与排序、gzip 存储、源命令、批量添加、详情模型、Tracker 热度、BT 交接、更新检查与应用状态流程。

## 目录结构

| 路径 | 说明 |
| --- | --- |
| `src-tauri/src/catalog.rs` | 目录解析、去重键、搜索、最新列表 |
| `src-tauri/src/storage.rs` | gzip 读写、原子写、源地址 sidecar |
| `src-tauri/src/commands.rs` | 源目录写入：下载、添加、更新、导入、删除、刷新 |
| `src-tauri/src/detail.rs` | 详情卡片与链接分组 |
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
