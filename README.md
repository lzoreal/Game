# GameMagNet

本地游戏目录管理器：把多个游戏资源站点的 JSON 目录合并成一个可搜索、可去重的本地库。

[![Release](https://img.shields.io/github/v/release/lzoreal/Game?label=release)](https://github.com/lzoreal/Game/releases/latest)
[![Build](https://github.com/lzoreal/Game/actions/workflows/build-release.yml/badge.svg)](https://github.com/lzoreal/Game/actions/workflows/build-release.yml)
[![Platform](https://img.shields.io/badge/platform-Windows-0078d4)](https://github.com/lzoreal/Game/releases/latest)
[![Python](https://img.shields.io/badge/python-3.12-3776ab)](https://www.python.org/)

## 功能特性

- **多源合并去重**：标题归一化后合并同一游戏的不同来源，链接集中展示。
- **空搜索默认列表**：搜索框为空时显示最新的 100 个游戏，大目录下也在后台线程计算，不阻塞界面。
- **模糊搜索**：支持关键词模糊匹配、相似度阈值、来源筛选与日期排序。
- **详情面板**：查看文件大小、上传日期、来源、链接类型，一键复制或发送到 BT 客户端。
- **源管理**：通过 URL 添加源、从本地导入 JSON、更新选中源、刷新全部源、删除源。
- **Cloudflare 兼容处理**：更新/下载时自动用真实浏览器 TLS 指纹重试，能通过只校验指纹的站点；遇到需要执行 JS 的质询会给出明确提示与导入引导。
- **检查更新**：从 GitHub Release 获取新版本信息，并在应用内提示下载。
- **明暗主题**：手动切换并记住选择，QML 界面自适应窗口尺寸。

## 下载

前往 [Releases](https://github.com/lzoreal/Game/releases/latest) 下载：

| 文件 | 说明 |
| --- | --- |
| `GameMagNet.exe` | 单文件免安装版，首次启动稍慢 |
| `GameMagNet-windows.zip` | 便携目录版，解压后运行 `GameMagNet/GameMagNet.exe` |

`GameMagNet.exe` 与 `sources/` 目录同级存放源文件，升级时保留该目录即可。

## 使用

1. 首次启动会自动创建 `sources/` 目录。
2. 通过「添加源」输入目录 JSON 的直链，或「从本地导入 JSON」导入已有文件。
3. 搜索框为空时浏览最新游戏；输入关键词后按回车或等待防抖自动搜索。
4. 单击结果在详情面板查看，双击直接打开详情；链接可一键复制或发送到 BT 客户端。
5. 「更新选中源」和「刷新全部源」会重新下载源 JSON，并记录源地址用于下次更新。

### Cloudflare 站点

部分源站点启用了 Cloudflare。更新/下载时会自动使用 `curl_cffi` 模拟 Chrome / Safari / Firefox 的 TLS 指纹重试：

- 只做指纹校验的站点会自动通过，无需额外操作。
- 需要浏览器执行 JavaScript 的托管质询（页面显示 “Just a moment...” / “请稍候…”）无法通过模拟请求自动绕过。应用仍会快速识别，并在错误对话框中提供「用浏览器打开」按钮和源地址。
- 遇到这种站点时，可以：
  1. 在浏览器中打开该地址完成验证，保存返回的 JSON，再通过「从本地导入 JSON」导入；
  2. 使用不受保护的镜像直链（例如内容相同、由其他站点托管的 `sources/*.json`）；
  3. 改用在浏览器中可正常访问的其他源。

## 从源码运行

```bash
python -m pip install -r requirements.txt
python main.py
```

## 构建 Windows 可执行文件

```bash
python -m pip install -r requirements-build.txt
python -m PyInstaller --noconfirm --clean GameMagNet.spec
```

产物：

- `dist/GameMagNet.exe`（onefile）
- `dist/GameMagNet/`（onedir，可压缩为便携包）

## 测试

```bash
python -m unittest discover -s tests -v
```

测试覆盖核心解析、搜索与排序、源命令、任务线程、QML 界面流程、Cloudflare 检测与下载回退。

## 目录结构

| 路径 | 说明 |
| --- | --- |
| `main.py` | 应用入口 |
| `app_backend.py` | QML 后端：状态、模型、任务调度 |
| `catalog.py` | 核心逻辑：存储、解析、搜索、最新列表、下载 |
| `search_controller.py` | 搜索防抖、相似度过滤与排序 |
| `source_commands.py` | 源的添加、导入、更新、刷新、删除 |
| `source_library.py` | 已加载源的库与统计 |
| `catalog_tasks.py` | 后台任务函数（加载、搜索、最新列表） |
| `task_runner.py` | 通用 Qt 后台线程与任务调度 |
| `detail_model.py` | 详情模型与链接分类 |
| `swarm_stats.py` | BT 磁力热度查询 |
| `bt_clients.py` | BT 客户端探测与发送计划 |
| `update_check.py` | GitHub Release 版本检查 |
| `theme_tokens.py` | 明暗主题色板与设置 |
| `qml/` | QML 界面与主题 |
| `tests/` | 单元测试与 QML 离屏测试 |

## 贡献

欢迎提交 Issue 和 Pull Request：

1. Fork 本仓库并从 `master` 创建分支，例如 `feat/source-filter`、`fix/cf-retry`。
2. 保持改动聚焦，遵循现有代码风格与中文界面文案。
3. 新增或修改逻辑时补充对应测试，并确保全量测试通过：
   ```bash
   python -m unittest discover -s tests -v
   ```
4. 在 PR 描述中说明动机、改动点和验证方式。

Bug 报告请尽量包含：操作系统版本、应用版本、重现步骤、错误提示或日志。

## 贡献者

感谢所有提交代码、反馈问题、改进文档的人。

<a href="https://github.com/lzoreal/Game/graphs/contributors">
  <img src="https://contrib.rocks/image?repo=lzoreal/Game" alt="Contributors" />
</a>

想出现在这里？欢迎从上面的贡献指南开始。

## 免责声明

本项目仅用于管理和检索公开的游戏目录 JSON 与磁力链接，不托管、不存储、不分发任何游戏文件。请遵守所在地区的法律法规以及各资源站点的使用条款。
