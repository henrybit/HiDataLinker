# HiDataLinker

[English](./README.md) | [中文](./README.zh-CN.md)

**面向 MySQL 与 PostgreSQL 的免费 MIT 桌面客户端。**

Navicat 风格的桌面数据库客户端：连接、浏览、查询、Dump 和迁移，凭证保存在本机。也可通过 MySQL 协议、使用与 MySQL 相同的连接类型连接 MariaDB，适合日常浏览和查询。MariaDB 并非单独文档化或测试的引擎，因此 MariaDB 专有特性可能覆盖不全。条目见 [MariaDB Ecosystem Hub](https://ecohub.mariadb.org/database-management/hidatalinker)。

**Windows · macOS · Linux** ｜ **MySQL · PostgreSQL** ｜ MariaDB 经 MySQL 协议 ｜ 中 / 英界面

## 下载

正式安装包在 [GitHub Releases](https://github.com/henrybit/HiDataLinker/releases)：

- macOS：`.dmg`（Apple Silicon / Intel）
- Windows：`.msi`
- Linux：`.deb` 与 AppImage

各版本能力见 [FEATURES.md](./FEATURES.md)。引擎范围与限制见 [官网文档](https://hidatalinker.onesolo.app/docs)。

---

## 愿景

传统数据库工具帮你「看见」表和字段；HiDataLinker 是面向 MySQL 与 PostgreSQL 日常工作的免费 MIT 工作台 —— 连接、浏览、查询、Dump 和迁移，凭证保存在本机。

可选的关系分析和智能体桥接是附加能力，不是产品承诺。分析可以使用你自己配置的大模型密钥；在文档明确之前，不要把计费 token 包或智能体任务当成已完全产品化的核心能力。

不是再做一个更全的 Navicat，而是：**面向 MySQL 与 PostgreSQL 的免费日常客户端，可选的分析和智能体工具会明确标为可选。**

---

## 如何使用

1. **下载安装** — 选择 Windows、macOS 或 Linux 版本，安装后即可使用，无需许可证。
2. **新建连接** — 选择 MySQL 或 PostgreSQL（MariaDB 使用与 MySQL 相同的连接类型），填写主机信息或粘贴连接 URL，测试通过后保存。
3. **浏览与操作** — 在对象树中展开库 / Schema，打开表查看数据与结构，用 SQL 编辑器查询、格式化、Explain。
4. **导出与迁移** — 需要备份或换环境时，Dump 为 `.sql`，或在迁移中心把同引擎库迁到目标连接。
5. **需要时再打开可选分析** — 运行「关系网分析」：选范围、可选上传业务文档（Markdown / Word / PDF），由大模型补注释、推断隐式关联，结果可导出 Markdown。智能体任务为可选。

日常查库改表在 MIT 许可下完全免费。可选的分析功能使用你自己配置的大模型密钥；在文档明确之前，不要把计费 token 包或智能体任务当成已完全产品化的核心能力。

---

## 支持的能力

### 基础管理（免费）

- 连接管理：保存 / 编辑 / 测试 / 连接，密码可选手动输入不落盘
- 对象浏览：库 / Schema、表、视图、索引、触发器、函数与存储过程
- 数据操作：分页预览、表格 / 表单视图、行插入 / 编辑 / 删除
- 结构与 DDL：列级修改、查看建表语句
- SQL 工作台：执行、格式化、Explain，快捷键运行
- 库运维：创建 / 删除库或 Schema，Dump 导出，运行 SQL 文件
- 同引擎迁移：源 → 目标可视化迁移，进度与历史可追溯
- 多端与多语言：Windows / macOS / Linux 原生桌面版，中英切换
- MariaDB：使用与 MySQL 相同的连接类型，适合日常浏览和查询；MariaDB 专有特性可能覆盖不全

### 可选分析与智能体桥接

- **关系分析** — 关系网分析、缺失注释推断，以及结合上传文档做业务 grounding，使用你自己配置的大模型密钥
- **可选智能体桥接** — 本地智能体 / MCP 类能力为可选，默认关闭；浏览、查询、Dump 和迁移不依赖它们

连接与数据在本机处理；密钥仅保存在本地，并只发给你选择的模型服务。

---

## 与传统数据库管理工具的区别

| | 传统工具（如 Navicat、DBeaver） | HiDataLinker |
|---|-------------------------------|--------------|
| **收费方式** | 席位买断 / 订阅，或免费版砍功能 | 免费 MIT 日常客户端；可选分析使用你自己的大模型密钥 |
| **产品重心** | GUI 广度、同步、企业插件 | 面向 MySQL 与 PostgreSQL 的日常 GUI |
| **AI** | 多为附属或缺失 | 可选的关系分析，使用你自己的大模型密钥；日常 SQL 并不依赖它 |
| **智能体** | 基本没有 | 可选的本地 MCP；浏览、查询、Dump 和迁移不依赖它 |
| **平台** | 各家不同 | Windows、macOS 和 Linux |
| **上手** | 常先付钱或先选复杂版本 | 下载即用，无授权墙 |

一句话：

- Navicat 卖的是**专业 GUI 许可证**
- DBeaver 卖的是**引擎广度与企业版**
- HiDataLinker 是**免费 MIT 日常客户端**；可选的分析和智能体工具会明确标为可选

更完整的定位与竞品分析见 [PRODUCT_POSITIONING.md](./PRODUCT_POSITIONING.md)。

---

## 适合谁

- 个人与小团队：想用免费 MIT 客户端做 MySQL 与 PostgreSQL 的日常查库改表
- 需要通过 MySQL 协议浏览和查询 MariaDB 的人
- 混用 Windows、macOS 和 Linux 的研发运维：一套工具、体验一致
- 希望用自己的大模型密钥做可选关系分析，但不把它当成日常 SQL 必需能力的团队

---

## 开发说明（智能体 sidecar）

内置数据分析 Agent 的 LangGraph 编排跑在 Node 子进程 `agents-runtime`（由 Rust 经 stdio NDJSON 托管），工具与 LLM 密钥仍只在 Rust 侧。

```bash
pnpm agents-runtime:build   # 产出 agents-runtime/dist/main.mjs
pnpm tauri:dev              # beforeDevCommand 会先构建 runtime
```

发行包通过 Tauri `externalBin` 嵌入平台 Node（`pnpm fetch:bundled-node` → `src-tauri/binaries/node-<triple>`）。开发机也可直接用本机 Node；可用 `HIDATALINKER_NODE` 覆盖路径。macOS 从 Finder 启动时系统 PATH 往往不含 Homebrew/nvm，故打包版必须带上 bundled node。

对话消息按会话持久化在 `~/.hidatalinker/chats/<agentId>/<sessionId>/`（`meta.json`、`transcript.jsonl`、`checkpoint.json`）。旧版 `agents/<agentId>/sessions/` 会在启动时自动迁移过来。

连接 schema 元数据缓存在 `~/.hidatalinker/db/`（`index.json` + `schemas/<connectionId>/`）。软件启动与智能体对话时检查：超过 24 小时才重新拉取；连接失败会标记状态，连续失败超过 7 天会清除该连接缓存。

运行日志默认写入 `~/.hidatalinker/runtime/YYYY-MM-DD.log`（设置 → 基础设置可开关）；启动时清理，最多保留 7 天。

---

欢迎 Star、提 Issue 与建议。
